//! Policy engine behaviour: one test per rule firing and not firing, the two
//! built-in checks, rule combinations, and rule-order independence.

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use domain::engine::{
    ACTIVE_REFUND_EXISTS, Claims, Decision, FAIL_CLOSED, Facts, ItemFacts, OrderFacts, PriorClaim,
    decide,
};
use domain::policy::Policy;
use domain::prose::{CLOSER_LOOK_REASON, NOT_COVERED_REASON};
use domain::types::{Flag, OrderStatus, ReasonCategory, RequestState, Verdict};
use uuid::Uuid;

const DEFAULT_POLICY: &str = include_str!("../../../../policy/default-policy.json");

const FINAL_SALE: &str = r#"{"kind":"final_sale_not_refundable","enabled":true}"#;
const WINDOW_30: &str =
    r#"{"kind":"refund_window","enabled":true,"days":30,"scope":{"kind":"all"}}"#;
const ABOVE_500: &str = r#"{"kind":"human_review_above","enabled":true,"amount_cents":50000}"#;
const DAMAGED: &str = r#"{"kind":"damaged_or_incorrect_eligible","enabled":true}"#;
const REPEAT: &str =
    r#"{"kind":"repeat_claim_limit","enabled":true,"max_claims":2,"lookback_days":180}"#;
const CONFLICT: &str = r#"{"kind":"conflicting_claim_escalates","enabled":true}"#;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 25, 12, 0, 0).unwrap()
}

fn days_ago(n: i64) -> DateTime<Utc> {
    now() - TimeDelta::days(n)
}

/// A clean request: damaged lamp, $100.00, delivered 5 days ago, no history.
fn facts() -> Facts {
    Facts {
        now: now(),
        order: Some(OrderFacts {
            order_id: Uuid::from_u128(1),
            order_ref: "ORD-1".into(),
            placed_at: days_ago(8),
            delivered_at: Some(days_ago(5)),
            status: OrderStatus::Delivered,
            item: ItemFacts {
                order_item_id: Uuid::from_u128(2),
                name: "Table lamp".into(),
                category: "home".into(),
                amount_cents: 10_000,
                final_sale: false,
                has_active_refund: false,
            },
        }),
        claims: Claims {
            reason: Some(ReasonCategory::Damaged),
            ..Claims::default()
        },
        prior_claims: vec![],
        flags: vec![],
    }
}

fn with(f: impl FnOnce(&mut Facts)) -> Facts {
    let mut facts = facts();
    f(&mut facts);
    facts
}

fn item(f: &mut Facts) -> &mut ItemFacts {
    &mut f.order.as_mut().unwrap().item
}

fn policy(rules: &[&str]) -> Policy {
    Policy::parse(&format!(r#"{{"rules":[{}]}}"#, rules.join(","))).unwrap()
}

fn default_policy() -> Policy {
    Policy::parse(DEFAULT_POLICY).unwrap()
}

/// The verdict of the single rule in `rule`, or None when it did not fire.
fn fires(rule: &str, facts: &Facts) -> Option<Verdict> {
    let d = decide(&policy(&[rule]), facts);
    assert!(d.fired.len() <= 1);
    d.fired.first().map(|f| f.verdict)
}

fn kinds(d: &Decision) -> Vec<&str> {
    d.fired.iter().map(|f| f.kind.as_str()).collect()
}

// ---- one rule at a time -------------------------------------------------------

#[test]
fn final_sale_denies_only_final_sale_items() {
    assert_eq!(fires(FINAL_SALE, &facts()), None);
    let sale = with(|f| item(f).final_sale = true);
    assert_eq!(fires(FINAL_SALE, &sale), Some(Verdict::Denied));
}

#[test]
fn refund_window_counts_from_delivery_and_includes_the_last_day() {
    let at_limit = with(|f| f.order.as_mut().unwrap().delivered_at = Some(days_ago(30)));
    assert_eq!(fires(WINDOW_30, &at_limit), None);

    let past_limit = with(|f| {
        f.order.as_mut().unwrap().delivered_at = Some(days_ago(30) - TimeDelta::seconds(1))
    });
    assert_eq!(fires(WINDOW_30, &past_limit), Some(Verdict::Denied));

    // Ordered long ago but delivered recently: the delivery date is the anchor.
    let late_delivery = with(|f| {
        let o = f.order.as_mut().unwrap();
        o.placed_at = days_ago(40);
        o.delivered_at = Some(days_ago(5));
    });
    assert_eq!(fires(WINDOW_30, &late_delivery), None);
}

#[test]
fn refund_window_falls_back_to_order_date_without_a_delivery() {
    let undelivered = with(|f| {
        let o = f.order.as_mut().unwrap();
        o.placed_at = days_ago(31);
        o.delivered_at = None;
        o.status = OrderStatus::Shipped;
    });
    let d = decide(&policy(&[WINDOW_30]), &undelivered);
    assert_eq!(d.verdict, Verdict::Denied);
    assert_eq!(d.fired[0].detail["anchor"], "placed_at");
    assert_eq!(d.fired[0].detail["age_days"], 31);
}

#[test]
fn category_window_applies_only_to_that_category() {
    let electronics = r#"{"kind":"refund_window","enabled":true,"days":14,"scope":{"kind":"category","category":"Electronics"}}"#;
    let old = |category: &str| {
        with(|f| {
            f.order.as_mut().unwrap().delivered_at = Some(days_ago(20));
            item(f).category = category.into();
        })
    };
    assert_eq!(
        fires(electronics, &old("electronics")),
        Some(Verdict::Denied)
    );
    assert_eq!(fires(electronics, &old("home")), None);
}

#[test]
fn human_review_threshold_is_strictly_above() {
    let at = with(|f| item(f).amount_cents = 50_000);
    assert_eq!(fires(ABOVE_500, &at), None);
    let above = with(|f| item(f).amount_cents = 50_001);
    assert_eq!(fires(ABOVE_500, &above), Some(Verdict::Escalated));
}

#[test]
fn only_damaged_and_wrong_item_are_eligible() {
    for reason in ReasonCategory::ALL {
        let f = with(|f| f.claims.reason = Some(*reason));
        let expected = matches!(reason, ReasonCategory::Damaged | ReasonCategory::WrongItem)
            .then_some(Verdict::Approved);
        assert_eq!(fires(DAMAGED, &f), expected, "{reason}");
    }
    let no_reason = with(|f| f.claims.reason = None);
    assert_eq!(fires(DAMAGED, &no_reason), None);
}

#[test]
fn repeat_claims_count_any_state_inside_the_lookback() {
    let claims = |ages: &[i64]| {
        with(|f| {
            f.prior_claims = ages
                .iter()
                .zip(
                    [RequestState::Denied, RequestState::ResolvedApproved]
                        .iter()
                        .cycle(),
                )
                .map(|(age, state)| PriorClaim {
                    decided_at: days_ago(*age),
                    state: *state,
                })
                .collect();
        })
    };
    assert_eq!(fires(REPEAT, &claims(&[10])), None);
    assert_eq!(fires(REPEAT, &claims(&[10, 180])), Some(Verdict::Escalated));
    assert_eq!(fires(REPEAT, &claims(&[10, 181])), None);

    // Needs no order: repeat claimants are escalated even before an order is known.
    let mut no_order = claims(&[1, 2]);
    no_order.order = None;
    assert_eq!(fires(REPEAT, &no_order), Some(Verdict::Escalated));
}

#[test]
fn conflicting_claims_escalate_with_the_matched_conditions() {
    let not_received = with(|f| f.claims.reason = Some(ReasonCategory::NotReceived));
    let d = decide(&policy(&[CONFLICT]), &not_received);
    assert_eq!(d.verdict, Verdict::Escalated);
    assert_eq!(
        d.fired[0].detail["conditions"],
        serde_json::json!(["not_received_but_delivered"])
    );

    let in_transit = with(|f| {
        f.claims.reason = Some(ReasonCategory::NotReceived);
        let o = f.order.as_mut().unwrap();
        o.status = OrderStatus::Shipped;
        o.delivered_at = None;
    });
    assert_eq!(fires(CONFLICT, &in_transit), None);

    let overclaim = with(|f| f.claims.claimed_amount_cents = Some(10_001));
    assert_eq!(fires(CONFLICT, &overclaim), Some(Verdict::Escalated));
    let exact_claim = with(|f| f.claims.claimed_amount_cents = Some(10_000));
    assert_eq!(fires(CONFLICT, &exact_claim), None);

    let contradictory = with(|f| {
        f.claims.contradictory_statements = true;
        f.order = None;
    });
    assert_eq!(fires(CONFLICT, &contradictory), Some(Verdict::Escalated));
    assert_eq!(fires(CONFLICT, &facts()), None);
}

// ---- built-in checks ---------------------------------------------------------

#[test]
fn any_flag_fails_closed_even_with_every_rule_disabled() {
    let all_off = Policy::parse(&DEFAULT_POLICY.replace("true", "false")).unwrap();
    let flagged = with(|f| {
        f.flags = vec![
            Flag::LowConfidence,
            Flag::PrescanSignal,
            Flag::LowConfidence,
        ];
    });
    let d = decide(&all_off, &flagged);
    assert_eq!(d.verdict, Verdict::Escalated);
    assert_eq!(kinds(&d), [FAIL_CLOSED]);
    assert_eq!(d.flags, [Flag::PrescanSignal, Flag::LowConfidence]);
    assert_eq!(d.customer_reasons(), [CLOSER_LOOK_REASON]);
    assert!(!d.fired[0].customer_reason.to_lowercase().contains("flag"));
}

#[test]
fn an_item_with_an_approved_refund_is_never_approved_again() {
    let refunded = with(|f| item(f).has_active_refund = true);
    // The conflicting-claims rule is absent, and the item is otherwise eligible.
    let d = decide(&policy(&[DAMAGED]), &refunded);
    assert_eq!(d.verdict, Verdict::Escalated);
    assert_eq!(
        kinds(&d),
        [ACTIVE_REFUND_EXISTS, "damaged_or_incorrect_eligible"]
    );
}

#[test]
fn nothing_fired_escalates_as_not_covered() {
    let changed_mind = with(|f| f.claims.reason = Some(ReasonCategory::ChangedMind));
    let d = decide(&default_policy(), &changed_mind);
    assert_eq!(d.verdict, Verdict::Escalated);
    assert!(d.fired.is_empty());
    assert_eq!(d.flags, [Flag::NoRuleFired]);
    assert_eq!(d.customer_reasons(), [NOT_COVERED_REASON]);
}

#[test]
fn rules_that_need_an_order_skip_when_there_is_none() {
    let no_order = with(|f| f.order = None);
    let d = decide(&default_policy(), &no_order);
    assert!(d.fired.is_empty(), "{:?}", kinds(&d));
    assert_eq!(d.verdict, Verdict::Escalated);
}

#[test]
fn disabled_rules_never_fire() {
    let all_off = Policy::parse(&DEFAULT_POLICY.replace("true", "false")).unwrap();
    let everything_wrong = with(|f| {
        item(f).final_sale = true;
        item(f).amount_cents = 999_999;
        f.order.as_mut().unwrap().delivered_at = Some(days_ago(400));
        f.claims.contradictory_statements = true;
    });
    let d = decide(&all_off, &everything_wrong);
    assert!(d.fired.is_empty());
    assert_eq!(d.flags, [Flag::NoRuleFired]);
}

// ---- combinations under the default policy ----------------------------------

#[test]
fn clean_damaged_item_is_approved() {
    let d = decide(&default_policy(), &facts());
    assert_eq!(d.verdict, Verdict::Approved);
    assert_eq!(kinds(&d), ["damaged_or_incorrect_eligible"]);
    assert!(d.flags.is_empty());
}

#[test]
fn damaged_but_final_sale_is_denied_with_only_the_denial_reason() {
    let d = decide(&default_policy(), &with(|f| item(f).final_sale = true));
    assert_eq!(d.verdict, Verdict::Denied);
    assert_eq!(
        kinds(&d),
        ["final_sale_not_refundable", "damaged_or_incorrect_eligible"]
    );
    assert_eq!(
        d.customer_reasons(),
        ["Items marked as final sale at purchase cannot be refunded."]
    );
}

#[test]
fn damaged_but_above_threshold_is_escalated() {
    let d = decide(&default_policy(), &with(|f| item(f).amount_cents = 129_900));
    assert_eq!(d.verdict, Verdict::Escalated);
    assert_eq!(
        kinds(&d),
        ["human_review_above", "damaged_or_incorrect_eligible"]
    );
}

#[test]
fn damaged_but_outside_the_window_is_denied() {
    let expired = with(|f| f.order.as_mut().unwrap().delivered_at = Some(days_ago(41)));
    let d = decide(&default_policy(), &expired);
    assert_eq!(d.verdict, Verdict::Denied);
    assert_eq!(
        kinds(&d),
        ["refund_window", "damaged_or_incorrect_eligible"]
    );
}

#[test]
fn denial_outranks_review_when_both_fire() {
    let d = decide(
        &default_policy(),
        &with(|f| {
            item(f).final_sale = true;
            item(f).amount_cents = 129_900;
        }),
    );
    assert_eq!(d.verdict, Verdict::Denied);
    assert_eq!(d.fired[0].verdict, Verdict::Denied, "most severe first");
}

#[test]
fn a_flag_holds_a_denial_for_a_person() {
    let flags = Flag::ALL
        .iter()
        .filter(|f| !matches!(f, Flag::ResponderFailure | Flag::NoRuleFired));
    for &flag in flags {
        let d = decide(
            &default_policy(),
            &with(|f| {
                item(f).final_sale = true;
                f.flags = vec![flag];
            }),
        );
        assert_eq!(d.verdict, Verdict::Escalated, "{flag}");
        // The trace still shows the denial, so the admin can deny in one step.
        assert_eq!(d.fired[0].kind, "final_sale_not_refundable", "{flag}");
        assert_eq!(d.fired[0].verdict, Verdict::Denied, "{flag}");
        assert_eq!(d.customer_reasons(), [CLOSER_LOOK_REASON], "{flag}");
    }
}

#[test]
fn a_responder_failure_alone_keeps_a_denial() {
    let failed = |flags: Vec<Flag>| {
        decide(
            &default_policy(),
            &with(|f| {
                item(f).final_sale = true;
                f.flags = flags;
            }),
        )
    };
    let d = failed(vec![Flag::ResponderFailure]);
    assert_eq!(d.verdict, Verdict::Denied);
    assert_eq!(
        d.customer_reasons(),
        ["Items marked as final sale at purchase cannot be refunded."]
    );
    let d = failed(vec![Flag::ResponderFailure, Flag::LowConfidence]);
    assert_eq!(
        d.verdict,
        Verdict::Escalated,
        "any other flag still holds it"
    );
}

#[test]
fn rule_order_never_changes_the_decision() {
    let busy = with(|f| {
        item(f).final_sale = true;
        item(f).category = "Accessories".into();
        item(f).amount_cents = 60_000;
        f.order.as_mut().unwrap().delivered_at = Some(days_ago(45));
        f.claims.contradictory_statements = true;
        f.prior_claims = vec![
            PriorClaim {
                decided_at: days_ago(3),
                state: RequestState::Denied,
            },
            PriorClaim {
                decided_at: days_ago(9),
                state: RequestState::Escalated,
            },
        ];
        f.flags = vec![Flag::LowConfidence];
    });
    let base = default_policy();
    let expected = decide(&base, &busy);
    assert_eq!(expected.fired.len(), 8, "every rule and fail_closed fire");

    let mut count = 0;
    for_each_permutation(&mut base.rules.clone(), 0, &mut |rules| {
        let shuffled = Policy {
            rules: rules.to_vec(),
        };
        assert_eq!(decide(&shuffled, &busy), expected);
        count += 1;
    });
    assert_eq!(count, 5040);
}

fn for_each_permutation<T: Clone>(items: &mut [T], k: usize, f: &mut impl FnMut(&[T])) {
    if k == items.len() {
        f(items);
        return;
    }
    for i in k..items.len() {
        items.swap(k, i);
        for_each_permutation(items, k + 1, f);
        items.swap(k, i);
    }
}

// ---- audit contract ----------------------------------------------------------

#[test]
fn facts_and_trace_round_trip_through_json() {
    // Stored as JSONB in decision_audit.facts and decision_audit.rule_trace.
    let f = with(|f| f.flags = vec![Flag::LowConfidence]);
    let d = decide(&default_policy(), &f);
    let facts_back: Facts = serde_json::from_value(serde_json::to_value(&f).unwrap()).unwrap();
    let trace_back: Vec<domain::engine::FiredRule> =
        serde_json::from_value(serde_json::to_value(&d.fired).unwrap()).unwrap();
    assert_eq!(facts_back, f);
    assert_eq!(trace_back, d.fired);
}
