//! The per-message pipeline (ADR-002, ADR-003). Runs in a spawned task after
//! the customer message is stored:
//!
//! 1. Take the conversation lock. A conversation that already has its request
//!    gets a holding reply; the verdict never changes (ADR-021).
//! 2. Window pre-scan. Any signal skips intake and fails closed.
//! 3. Intake (LLM, with one fallback model) extracts claims. Rust then checks
//!    every id against the customer's own orders and raises flags.
//! 4. A customer who is done, an off-topic message, or an item that already
//!    has a request gets a reply that files nothing. Otherwise, missing order,
//!    item or reason: ask a clarifying question, up to `MAX_CLARIFY_TURNS`,
//!    then escalate. Low confidence waits for the same point: it only
//!    escalates a complete request or one out of questions.
//! 5. `domain::engine::decide` returns the verdict. The responder only words it,
//!    and a reply that names another outcome is rejected.
//! 6. Request, audit, event and reply commit in one transaction.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::time::{Duration, Instant};

use ai::{AiConfig, AiError, Completed, Stage, StageModel, StageRecord};
use chrono::{DateTime, Utc};
use db::DbError;
use db::messages::{Message, SignalRow};
use db::orders::{Order, OrderItem};
use db::refunds::{NewAudit, NewRefundRequest};
use domain::engine::{Claims, Facts, ItemFacts, OrderFacts, PriorClaim, decide};
use domain::intake::{
    CustomerMessage, ExistingRequest, IntakeInput, IntakeOutput, IntakeStatus, Intent, ItemSummary,
    MAX_CLARIFY_TURNS, MissingField, OrderSummary,
};
use domain::prescan::{WindowMessage, prescan_window};
use domain::prose::render_policy;
use domain::responder::{
    PriorRequest, ResponderInput, Target, clean_reply, fallback_reply, holding_reply,
    validate_reply,
};
use domain::types::{AssistantKind, Flag, MessageRole, RequestState, SignalScope, Verdict};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::AppState;
use crate::review_job;
use crate::sse::{Emitter, SseEvent};

/// How one LLM stage went, including failed attempts. Stored in
/// `decision_audit.stages`.
#[derive(Clone, Debug, Default, Serialize)]
pub struct StageLog {
    pub record: Option<StageRecord>,
    pub failures: Vec<StageFailure>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StageFailure {
    pub model: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, Serialize)]
struct Stages {
    intake: Option<StageLog>,
    responder: Option<StageLog>,
}

/// Calls the stage's model, then the fallback model once. Each attempt is
/// bounded by the stage timeout whatever the assistant implementation does.
pub async fn call_with_fallback<T, F, Fut>(
    ai: &AiConfig,
    stage: Stage,
    call: F,
) -> (Option<T>, StageLog)
where
    F: Fn(StageModel) -> Fut,
    Fut: Future<Output = Result<Completed<T>, AiError>>,
{
    let mut log = StageLog::default();
    for (attempt, model) in [(1, ai.stage(stage).clone()), (2, ai.fallback_for(stage))] {
        let name = model.model.clone();
        let secs = model.timeout_secs;
        let started = Instant::now();
        let result = tokio::time::timeout(Duration::from_secs(secs), call(model))
            .await
            .unwrap_or(Err(AiError::Timeout(secs)));
        match result {
            Ok(Completed { output, mut record }) => {
                record.attempt = attempt;
                record.fallback = attempt > 1;
                record.latency_ms =
                    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                log.record = Some(record);
                return (Some(output), log);
            }
            Err(e) => {
                tracing::warn!(stage = stage.as_str(), model = %name, error = %e, "LLM call failed");
                log.failures.push(StageFailure {
                    model: name,
                    error: e.to_string(),
                });
            }
        }
    }
    (None, log)
}

/// The order and item an intake result points at, if both are the customer's
/// own. A single-item order needs no item id.
pub fn resolve_target<'a>(
    orders: &'a [Order],
    intake: &IntakeOutput,
) -> Option<(&'a Order, &'a OrderItem)> {
    let order = orders.iter().find(|o| Some(o.id) == intake.order_id)?;
    let item = match intake.order_item_id.and_then(|id| order.item(id)) {
        Some(item) => item,
        None if order.items.len() == 1 => &order.items[0],
        None => return None,
    };
    Some((order, item))
}

/// Database rows plus (untrusted) intake claims → the engine's input. Pure, so
/// scenario tests and eval tools can reuse it.
pub fn build_facts(
    now: DateTime<Utc>,
    orders: &[Order],
    prior_claims: Vec<PriorClaim>,
    intake: Option<&IntakeOutput>,
    flags: Vec<Flag>,
) -> Facts {
    let order = intake
        .and_then(|i| resolve_target(orders, i))
        .map(|(o, item)| OrderFacts {
            order_id: o.id,
            order_ref: o.order_ref.clone(),
            placed_at: o.placed_at,
            delivered_at: o.delivered_at,
            status: o.status,
            item: ItemFacts {
                order_item_id: item.id,
                name: item.name.clone(),
                category: item.category.clone(),
                amount_cents: item.amount_cents,
                final_sale: item.final_sale,
                has_active_refund: item.active_refund,
            },
        });
    let claims = intake
        .map(|i| Claims {
            reason: i.reason_category,
            claimed_amount_cents: i.claimed_amount_cents,
            contradictory_statements: i.contradictory_statements,
        })
        .unwrap_or_default();
    Facts {
        now,
        order,
        claims,
        prior_claims,
        flags,
    }
}

/// Flags raised by checking intake output against the database. The model's
/// ids and claims are never trusted on their own.
pub async fn screen_intake(
    db: &db::Db,
    customer_id: Uuid,
    orders: &[Order],
    intake: &IntakeOutput,
) -> Result<Vec<Flag>, DbError> {
    let mut flags = Vec::new();
    let own: HashSet<&str> = orders.iter().map(|o| o.order_ref.as_str()).collect();
    let foreign: Vec<String> = intake
        .mentioned_order_refs
        .iter()
        .map(|r| r.trim().to_uppercase())
        .filter(|r| !own.contains(r.as_str()))
        .collect();
    if !foreign.is_empty()
        && db::orders::owners_by_ref(db, &foreign)
            .await?
            .iter()
            .any(|(_, owner)| *owner != customer_id)
    {
        flags.push(Flag::ForeignOrderReference);
    }
    if !intake.injection_signals.is_empty() {
        flags.push(Flag::IntakeInjectionSignal);
    }
    // An incomplete request is expected to be unclear; the clarifying
    // questions deal with it, and `process` adds the flag if they run out.
    if intake.low_confidence() && missing_fields(orders, intake).is_empty() {
        flags.push(Flag::LowConfidence);
    }
    Ok(flags)
}

/// What is still unknown after intake, or empty when the request can be decided.
pub fn missing_fields(orders: &[Order], intake: &IntakeOutput) -> Vec<MissingField> {
    let mut missing = intake.missing.clone();
    if intake.status == IntakeStatus::NeedsInfo && missing.is_empty() {
        missing.extend([
            MissingField::Order,
            MissingField::Item,
            MissingField::Reason,
        ]);
    }
    if intake
        .order_id
        .is_none_or(|id| !orders.iter().any(|o| o.id == id))
    {
        missing.push(MissingField::Order);
    }
    if resolve_target(orders, intake).is_none() {
        missing.push(MissingField::Item);
    }
    if intake.reason_category.is_none() {
        missing.push(MissingField::Reason);
    }
    missing.sort();
    missing.dedup();
    missing
}

/// The reply for a message that files no request, if it is one: the customer
/// is done, the message is off-topic, or the item it is about already has a
/// request in any state.
fn no_request_reply(
    orders: &[Order],
    item_requests: &HashMap<Uuid, ExistingRequest>,
    intake: &IntakeOutput,
) -> Option<(AssistantKind, ResponderInput)> {
    match intake.intent {
        Intent::Finished => return Some((AssistantKind::Closing, ResponderInput::Closing)),
        Intent::OutOfScope => return Some((AssistantKind::Redirect, ResponderInput::Redirect)),
        Intent::RefundRequest => {}
    }
    let (order, item) = resolve_target(orders, intake)?;
    let existing = item_requests.get(&item.id)?;
    let request = PriorRequest::new(
        &existing.request_ref,
        &order.order_ref,
        &item.name,
        existing.state,
        existing.decided_at,
    );
    Some((
        AssistantKind::ExistingRequest,
        ResponderInput::ExistingRequest { request },
    ))
}

fn order_summaries(
    orders: &[Order],
    item_requests: &HashMap<Uuid, ExistingRequest>,
) -> Vec<OrderSummary> {
    orders
        .iter()
        .map(|o| OrderSummary {
            id: o.id,
            order_ref: o.order_ref.clone(),
            placed_at: o.placed_at,
            delivered_at: o.delivered_at,
            status: o.status,
            items: o
                .items
                .iter()
                .map(|i| ItemSummary {
                    id: i.id,
                    name: i.name.clone(),
                    category: i.category.clone(),
                    amount_cents: i.amount_cents,
                    final_sale: i.final_sale,
                    existing_request: item_requests.get(&i.id).cloned(),
                })
                .collect(),
        })
        .collect()
}

/// Entry point for the spawned task. Always ends the stream with `done`;
/// unexpected errors are logged and reported as `error{code:"internal"}`.
pub async fn run_message(
    state: AppState,
    conversation_id: Uuid,
    customer_id: Uuid,
    message: Message,
    out: Emitter,
) {
    let outcome = match db::lock::lock_conversation(&state.db, conversation_id).await {
        Ok(lock) => {
            let outcome = process(&state, conversation_id, customer_id, &message, &out).await;
            if let Err(e) = lock.release().await {
                tracing::warn!(error = %e, "conversation lock release failed");
            }
            outcome
        }
        Err(e) => Err(e.into()),
    };
    match outcome {
        Ok(Some(escalated_request)) => {
            tokio::spawn(review_job::run_review(state.clone(), escalated_request));
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = ?e, %conversation_id, "message pipeline failed");
            out.error("internal", "Something went wrong. Please try again.")
                .await;
        }
    }
    out.send(SseEvent::Done).await;
}

/// Returns the request id when a new escalation needs a review draft.
async fn process(
    state: &AppState,
    conversation_id: Uuid,
    customer_id: Uuid,
    message: &Message,
    out: &Emitter,
) -> anyhow::Result<Option<Uuid>> {
    let db = &state.db;
    if let Some(existing) = db::refunds::find_request_for_conversation(db, conversation_id).await? {
        // Folded into a decision made while this message waited for the lock.
        if message.seq <= existing.evaluated_through_seq.unwrap_or(0) {
            return Ok(None);
        }
        let body = holding_reply(&existing.request_ref, existing.state);
        let mut conn = db.0.acquire().await?;
        let reply = db::messages::insert_assistant_message(
            &mut conn,
            conversation_id,
            AssistantKind::Holding,
            &body,
        )
        .await?;
        out.reply(&reply).await;
        return Ok(None);
    }

    let mut reran = false;
    loop {
        let all = db::messages::list_messages(db, conversation_id).await?;
        let loaded_through = all.last().map_or(0, |m| m.seq);
        let customer: Vec<&Message> = all
            .iter()
            .filter(|m| m.role == MessageRole::Customer)
            .collect();
        let signals = window_prescan(state, conversation_id, &customer).await?;
        let orders = db::orders::list_orders_for_customer(db, customer_id).await?;
        let item_requests = db::refunds::item_requests(db, customer_id).await?;

        let mut flags = Vec::new();
        let mut stages = Stages::default();
        let mut intake = None;
        if !signals.is_empty() {
            flags.push(Flag::PrescanSignal);
        } else {
            let input = IntakeInput {
                orders: order_summaries(&orders, &item_requests),
                selected_order_id: customer.iter().rev().find_map(|m| m.order_id),
                messages: customer
                    .iter()
                    .map(|m| CustomerMessage {
                        id: m.id,
                        seq: m.seq,
                        body: m.body.clone(),
                    })
                    .collect(),
            };
            let assistant = &state.assistant;
            let input = &input;
            let (result, log) = call_with_fallback(&state.ai, Stage::Intake, |m| async move {
                assistant.intake(input, &m).await
            })
            .await;
            stages.intake = Some(log);
            match result {
                None => flags.push(Flag::LlmFailure),
                Some(output) => {
                    flags.extend(screen_intake(db, customer_id, &orders, &output).await?);
                    intake = Some(output);
                }
            }
        }

        if flags.is_empty()
            && let Some(extracted) = &intake
        {
            // Not a clarify turn: a model failure falls back to the template
            // rather than escalating, since nothing is being decided.
            if let Some((kind, input)) = no_request_reply(&orders, &item_requests, extracted) {
                let (reply, _) = respond(state, &input).await;
                let body = reply.unwrap_or_else(|| fallback_reply(&input.expectation(), None));
                let mut conn = db.0.acquire().await?;
                let reply =
                    db::messages::insert_assistant_message(&mut conn, conversation_id, kind, &body)
                        .await?;
                out.reply(&reply).await;
                return Ok(None);
            }
            let missing = missing_fields(&orders, extracted);
            if !missing.is_empty() {
                let asked = db::messages::clarify_count(db, conversation_id).await?;
                if asked >= i64::from(MAX_CLARIFY_TURNS) {
                    flags.push(Flag::ClarificationLimit);
                    if extracted.low_confidence() {
                        flags.push(Flag::LowConfidence);
                    }
                } else {
                    let policy = db::policy::latest_policy(db).await?;
                    let input = ResponderInput::Clarify {
                        missing,
                        clarify_turn: u8::try_from(asked + 1).unwrap_or(u8::MAX),
                        policy_prose: render_policy(&policy.rules),
                    };
                    let (reply, _) = respond(state, &input).await;
                    match reply {
                        Some(body) => {
                            let mut conn = db.0.acquire().await?;
                            let reply = db::messages::insert_assistant_message(
                                &mut conn,
                                conversation_id,
                                AssistantKind::Clarify,
                                &body,
                            )
                            .await?;
                            out.reply(&reply).await;
                            return Ok(None);
                        }
                        None => flags.push(Flag::LlmFailure),
                    }
                }
            }
        }

        // Messages that arrived during the LLM calls are waiting on the lock;
        // fold them into this decision once rather than decide without them.
        if !reran && db::conversations::last_seq(db, conversation_id).await? > loaded_through {
            reran = true;
            continue;
        }

        let evaluated_through_seq = customer.last().map_or(0, |m| m.seq);
        return decide_and_reply(
            state,
            conversation_id,
            customer_id,
            DecisionInputs {
                orders,
                intake,
                flags,
                stages,
                signals,
                evaluated_through_seq,
            },
            out,
        )
        .await;
    }
}

/// Scans the recent customer messages together, stores any new window
/// signals, and returns every signal in the conversation.
async fn window_prescan(
    state: &AppState,
    conversation_id: Uuid,
    customer: &[&Message],
) -> anyhow::Result<Vec<SignalRow>> {
    let window: Vec<WindowMessage> = customer
        .iter()
        .map(|m| WindowMessage {
            id: m.id,
            text: &m.body,
        })
        .collect();
    let hits = prescan_window(&window);
    if !hits.is_empty() {
        let mut conn = state.db.0.acquire().await?;
        for hit in &hits {
            db::messages::insert_signals(
                &mut conn,
                hit.message_id,
                SignalScope::Window,
                std::slice::from_ref(&hit.signal),
            )
            .await?;
        }
    }
    Ok(db::messages::list_signals_for_conversation(&state.db, conversation_id).await?)
}

/// Responder with fallback; a reply that fails validation counts as a failed
/// call. `None` means both models failed.
async fn respond(state: &AppState, input: &ResponderInput) -> (Option<String>, StageLog) {
    let expectation = &input.expectation();
    let assistant = &state.assistant;
    call_with_fallback(&state.ai, Stage::Responder, |m| async move {
        let mut done = assistant.respond(input, &m).await?;
        done.output = clean_reply(&done.output);
        validate_reply(&done.output, expectation).map_err(|v| AiError::Rejected(v.0))?;
        Ok(done)
    })
    .await
}

struct DecisionInputs {
    orders: Vec<Order>,
    intake: Option<IntakeOutput>,
    flags: Vec<Flag>,
    stages: Stages,
    signals: Vec<SignalRow>,
    evaluated_through_seq: i32,
}

async fn decide_and_reply(
    state: &AppState,
    conversation_id: Uuid,
    customer_id: Uuid,
    inputs: DecisionInputs,
    out: &Emitter,
) -> anyhow::Result<Option<Uuid>> {
    let db = &state.db;
    let DecisionInputs {
        orders,
        intake,
        flags,
        mut stages,
        signals,
        evaluated_through_seq,
    } = inputs;
    let policy = db::policy::latest_policy(db).await?;
    let prior = db::refunds::prior_claims(db, customer_id, conversation_id).await?;
    let mut facts = build_facts(Utc::now(), &orders, prior, intake.as_ref(), flags);
    let mut decision = decide(&policy.rules, &facts);
    let target = facts
        .order
        .as_ref()
        .map(|o| Target::new(&o.order_ref, &o.item.name, o.item.amount_cents));
    let policy_prose = render_policy(&policy.rules);
    let verdict_input = |verdict: Verdict, reasons: Vec<String>| ResponderInput::Verdict {
        verdict,
        target: target.clone(),
        reasons,
        policy_prose: policy_prose.clone(),
    };

    let input = verdict_input(decision.verdict, decision.customer_reasons());
    let (reply, log) = respond(state, &input).await;
    stages.responder = Some(log);
    let body = match reply {
        Some(body) => body,
        None => {
            // The failure is a flag like any other, so the engine applies it
            // (ADR-030) and the audit trace shows why; the template words it.
            facts.flags.push(Flag::ResponderFailure);
            decision = decide(&policy.rules, &facts);
            let input = verdict_input(decision.verdict, decision.customer_reasons());
            fallback_reply(&input.expectation(), input.target())
        }
    };

    let order = facts.order.as_ref();
    let request = NewRefundRequest {
        conversation_id,
        customer_id,
        order_id: order.map(|o| o.order_id),
        order_item_id: order.map(|o| o.item.order_item_id),
        amount_cents: order.map(|o| o.item.amount_cents),
        reason_category: facts.claims.reason,
        state: RequestState::from(decision.verdict),
    };
    let audit = NewAudit {
        policy_version_id: policy.meta.id,
        content_hash: policy.meta.content_hash.clone(),
        evaluated_through_seq,
        verdict: decision.verdict,
        prescan_signals: json!(signals),
        extracted: intake.as_ref().map(|i| json!(i)),
        facts: json!(facts),
        rule_trace: json!(decision.fired),
        flags: json!(decision.flags),
        stages: json!(stages),
        fired_kinds: decision.fired.iter().map(|f| f.kind.clone()).collect(),
    };

    let mut tx = db.0.begin().await?;
    let created = match db::refunds::create_decided(&mut tx, &request, &audit).await {
        Ok(created) => created,
        Err(DbError::Conflict(code @ "duplicate_active_refund")) => {
            out.error(
                code,
                "This item already has a refund in progress. Please refresh the page.",
            )
            .await;
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };
    let reply = db::messages::insert_assistant_message(
        &mut tx,
        conversation_id,
        AssistantKind::Verdict,
        &body,
    )
    .await?;
    tx.commit().await?;

    tracing::info!(
        request = %created.request_ref,
        verdict = %decision.verdict,
        flags = ?decision.flags,
        "refund request decided"
    );
    out.reply(&reply).await;
    if let Some(summary) = db::conversations::find_request_summary(db, conversation_id).await? {
        out.send(SseEvent::RequestUpdated(summary)).await;
    }
    Ok((decision.verdict == Verdict::Escalated).then_some(created.id))
}
