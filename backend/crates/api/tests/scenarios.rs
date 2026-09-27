//! The seeded scenario matrix, decided without an LLM: each scenario's opening
//! messages go through the real pre-scan; if they pass, the intake output a
//! correct model would return goes through the real screening, `build_facts`
//! and `decide` with the seeded policy. The verdict and flags must match what
//! the README promises.

use std::collections::HashMap;

use api::pipeline::{build_facts, screen_intake};
use chrono::Utc;
use db::seed::{SCENARIOS, Scenario, item_id, stable_id};
use db::{Db, seed};
use domain::engine::decide;
use domain::intake::{IntakeOutput, IntakeStatus, Intent};
use domain::prescan::{WindowMessage, prescan, prescan_window};
use domain::types::{Flag, ReasonCategory};
use sqlx::PgPool;
use uuid::Uuid;

/// (reason, item index, typed order refs) that a correct intake extracts.
fn reading(key: &str) -> (ReasonCategory, usize, &'static [&'static str]) {
    use ReasonCategory::*;
    match key {
        "clean_wrong_item" => (WrongItem, 0, &["ORD-10362"]),
        "above_threshold" => (ChangedMind, 0, &[]),
        "conflicting_not_received" => (NotReceived, 0, &["ORD-10418"]),
        "cross_customer_attack" => (Damaged, 0, &["ORD-10388"]),
        "changed_mind" => (ChangedMind, 0, &["ORD-10409"]),
        "multi_item" => (Damaged, 0, &["ORD-10261"]),
        _ => (Damaged, 0, &[]),
    }
}

fn correct_intake(s: &Scenario) -> IntakeOutput {
    let (reason, item, typed) = reading(s.key);
    let mut mentioned: Vec<String> = typed.iter().map(|r| r.to_string()).collect();
    if let Some(target) = s.target_order_ref
        && !mentioned.iter().any(|r| r == target)
    {
        mentioned.push(target.to_owned());
    }
    IntakeOutput {
        intent: Intent::RefundRequest,
        status: IntakeStatus::Complete,
        missing: vec![],
        order_id: s.target_order_ref.map(|r| stable_id("order", r)),
        order_item_id: s.target_order_ref.map(|r| item_id(r, item)),
        mentioned_order_refs: mentioned,
        reason_category: Some(reason),
        claimed_amount_cents: None,
        contradictory_statements: false,
        injection_signals: vec![],
        confidence: 0.95,
    }
}

fn scanned(s: &Scenario) -> bool {
    let ids: Vec<Uuid> = (0..s.request_messages.len())
        .map(|i| Uuid::from_u128(i as u128 + 1))
        .collect();
    let window: Vec<WindowMessage> = ids
        .iter()
        .zip(s.request_messages)
        .map(|(id, text)| WindowMessage { id: *id, text })
        .collect();
    s.request_messages.iter().any(|m| !prescan(m).is_empty()) || !prescan_window(&window).is_empty()
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_scenario_gets_its_documented_verdict(pool: PgPool) {
    let db = Db(pool);
    seed::run(&db, api::DEFAULT_POLICY_JSON).await.unwrap();
    let policy = db::policy::latest_policy(&db).await.unwrap().rules;

    let mut outcomes = HashMap::new();
    for s in SCENARIOS {
        let customer_id = stable_id("customer", s.email);
        let orders = db::orders::list_orders_for_customer(&db, customer_id)
            .await
            .unwrap();
        let prior = db::refunds::prior_claims(&db, customer_id, Uuid::nil())
            .await
            .unwrap();

        let mut flags = Vec::new();
        let mut intake = None;
        if scanned(s) {
            flags.push(Flag::PrescanSignal);
        } else {
            let stub = correct_intake(s);
            flags.extend(
                screen_intake(&db, customer_id, &orders, &stub)
                    .await
                    .unwrap(),
            );
            intake = Some(stub);
        }
        let facts = build_facts(Utc::now(), &orders, prior, intake.as_ref(), flags);
        let decision = decide(&policy, &facts);
        outcomes.insert(s.key, (decision.verdict, decision.flags.clone()));

        assert_eq!(
            decision.verdict, s.expected_verdict,
            "{}: {decision:#?}",
            s.key
        );
        assert_eq!(decision.flags, s.expected_flags, "{}", s.key);
        if s.target_order_ref.is_some() && !decision.flags.contains(&Flag::PrescanSignal) {
            assert!(
                facts.order.is_some(),
                "{}: target order not resolved",
                s.key
            );
        }
    }
    assert_eq!(outcomes.len(), 15);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_winning_rule_matches_the_scenario(pool: PgPool) {
    let db = Db(pool);
    seed::run(&db, api::DEFAULT_POLICY_JSON).await.unwrap();
    let policy = db::policy::latest_policy(&db).await.unwrap().rules;
    let expected_top = [
        ("clean_damaged", "damaged_or_incorrect_eligible"),
        ("final_sale", "final_sale_not_refundable"),
        ("expired_window", "refund_window"),
        ("above_threshold", "human_review_above"),
        ("repeat_claimant", "repeat_claim_limit"),
        ("conflicting_not_received", "conflicting_claim_escalates"),
        ("already_refunded", "active_refund_exists"),
        ("damaged_but_expired", "refund_window"),
    ];
    for (key, kind) in expected_top {
        let s = SCENARIOS.iter().find(|s| s.key == key).unwrap();
        let customer_id = stable_id("customer", s.email);
        let orders = db::orders::list_orders_for_customer(&db, customer_id)
            .await
            .unwrap();
        let prior = db::refunds::prior_claims(&db, customer_id, Uuid::nil())
            .await
            .unwrap();
        let stub = correct_intake(s);
        let facts = build_facts(Utc::now(), &orders, prior, Some(&stub), vec![]);
        let decision = decide(&policy, &facts);
        assert_eq!(decision.fired[0].kind, kind, "{key}: {:#?}", decision.fired);
    }
}
