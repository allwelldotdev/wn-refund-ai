//! Refund requests, their decision audit, and escalation reviews.

use chrono::{DateTime, Utc};
use domain::engine::{Facts, FiredRule, PriorClaim};
use domain::intake::IntakeOutput;
use domain::policy::Policy;
use domain::types::{Flag, ReasonCategory, RequestState, Verdict};
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{Db, DbError, from_json, is_unique_violation, parse_enum};

pub struct NewRefundRequest {
    pub conversation_id: Uuid,
    pub customer_id: Uuid,
    pub order_id: Option<Uuid>,
    pub order_item_id: Option<Uuid>,
    pub amount_cents: Option<i64>,
    pub reason_category: Option<ReasonCategory>,
    pub state: RequestState,
}

/// Everything needed to explain the verdict later (ADR-022).
pub struct NewAudit {
    pub policy_version_id: Uuid,
    pub content_hash: String,
    pub evaluated_through_seq: i32,
    pub verdict: Verdict,
    pub prescan_signals: Value,
    pub extracted: Option<Value>,
    pub facts: Value,
    pub rule_trace: Value,
    pub flags: Value,
    pub stages: Value,
    /// For the `decided` event payload.
    pub fired_kinds: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreatedRequest {
    pub id: Uuid,
    pub request_ref: String,
    pub state: RequestState,
    pub created_at: DateTime<Utc>,
}

/// Inserts the request, its audit row and the `decided` event, plus a pending
/// escalation review when escalated. Runs inside the caller's transaction, so
/// the verdict reply can commit with it.
///
/// `Conflict("duplicate_active_refund")` when the item gained an approved
/// refund concurrently (ADR-013); `Conflict("request_exists")` when the
/// conversation already has its request (ADR-021).
pub async fn create_decided(
    conn: &mut PgConnection,
    req: &NewRefundRequest,
    audit: &NewAudit,
) -> Result<CreatedRequest, DbError> {
    let r = sqlx::query!(
        "INSERT INTO refund_requests
           (conversation_id, customer_id, order_id, order_item_id, amount_cents, reason_category, state)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, ref, created_at",
        req.conversation_id,
        req.customer_id,
        req.order_id,
        req.order_item_id,
        req.amount_cents,
        req.reason_category.map(|c| c.as_str()),
        req.state.as_str(),
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        if is_unique_violation(&e, "refund_requests_one_active_per_item") {
            DbError::Conflict("duplicate_active_refund")
        } else if is_unique_violation(&e, "refund_requests_conversation_id_key") {
            DbError::Conflict("request_exists")
        } else {
            e.into()
        }
    })?;

    sqlx::query!(
        "INSERT INTO decision_audit
           (refund_request_id, policy_version_id, content_hash, evaluated_through_seq, verdict,
            prescan_signals, extracted, facts, rule_trace, flags, stages)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        r.id,
        audit.policy_version_id,
        audit.content_hash,
        audit.evaluated_through_seq,
        audit.verdict.as_str(),
        audit.prescan_signals,
        audit.extracted,
        audit.facts,
        audit.rule_trace,
        audit.flags,
        audit.stages,
    )
    .execute(&mut *conn)
    .await?;

    let payload = json!({
        "verdict": audit.verdict,
        "flags": audit.flags,
        "fired_kinds": audit.fired_kinds,
    });
    sqlx::query!(
        "INSERT INTO request_events (refund_request_id, kind, actor_kind, payload)
         VALUES ($1, 'decided', 'system', $2)",
        r.id,
        payload,
    )
    .execute(&mut *conn)
    .await?;

    if req.state == RequestState::Escalated {
        sqlx::query!(
            "INSERT INTO escalation_reviews (refund_request_id, status) VALUES ($1, 'pending')",
            r.id,
        )
        .execute(&mut *conn)
        .await?;
    }

    Ok(CreatedRequest {
        id: r.id,
        request_ref: r.r#ref,
        state: req.state,
        created_at: r.created_at,
    })
}

/// The conversation's request and how far its decision read.
#[derive(Clone, Debug, PartialEq)]
pub struct ExistingRequest {
    pub id: Uuid,
    pub request_ref: String,
    pub state: RequestState,
    /// `None` for seeded history, which predates the engine.
    pub evaluated_through_seq: Option<i32>,
}

pub async fn find_request_for_conversation(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Option<ExistingRequest>, DbError> {
    let r = sqlx::query!(
        r#"SELECT r.id, r.ref, r.state, a.evaluated_through_seq AS "evaluated_through_seq?"
           FROM refund_requests r
           LEFT JOIN decision_audit a ON a.refund_request_id = r.id
           WHERE r.conversation_id = $1"#,
        conversation_id,
    )
    .fetch_optional(&db.0)
    .await?;
    r.map(|r| {
        Ok(ExistingRequest {
            id: r.id,
            request_ref: r.r#ref,
            state: parse_enum(&r.state)?,
            evaluated_through_seq: r.evaluated_through_seq,
        })
    })
    .transpose()
}

/// The customer's other requests, in any state, dated by creation.
pub async fn prior_claims(
    db: &Db,
    customer_id: Uuid,
    exclude_conversation: Uuid,
) -> Result<Vec<PriorClaim>, DbError> {
    let rows = sqlx::query!(
        "SELECT created_at, state FROM refund_requests
         WHERE customer_id = $1 AND conversation_id <> $2
         ORDER BY created_at",
        customer_id,
        exclude_conversation,
    )
    .fetch_all(&db.0)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(PriorClaim {
                decided_at: r.created_at,
                state: parse_enum(&r.state)?,
            })
        })
        .collect()
}

/// What the review model is shown for a pending escalation.
pub struct ReviewCase {
    pub request_ref: String,
    pub conversation_id: Uuid,
    pub evaluated_through_seq: i32,
    pub facts: Facts,
    pub extracted: Option<IntakeOutput>,
    pub rule_trace: Vec<FiredRule>,
    pub flags: Vec<Flag>,
    pub policy: Policy,
}

/// `None` unless the request has a review still pending.
pub async fn pending_review_case(
    db: &Db,
    refund_request_id: Uuid,
) -> Result<Option<ReviewCase>, DbError> {
    let r = sqlx::query!(
        "SELECT r.ref, r.conversation_id, a.evaluated_through_seq, a.facts, a.extracted,
                a.rule_trace, a.flags, p.rules
         FROM refund_requests r
         JOIN escalation_reviews v ON v.refund_request_id = r.id AND v.status = 'pending'
         JOIN decision_audit a ON a.refund_request_id = r.id
         JOIN policy_versions p ON p.id = a.policy_version_id
         WHERE r.id = $1",
        refund_request_id,
    )
    .fetch_optional(&db.0)
    .await?;
    r.map(|r| {
        Ok(ReviewCase {
            request_ref: r.r#ref,
            conversation_id: r.conversation_id,
            evaluated_through_seq: r.evaluated_through_seq,
            facts: from_json(r.facts)?,
            extracted: r.extracted.map(from_json).transpose()?,
            rule_trace: from_json(r.rule_trace)?,
            flags: from_json(r.flags)?,
            policy: from_json(r.rules)?,
        })
    })
    .transpose()
}

pub async fn list_pending_reviews(db: &Db) -> Result<Vec<Uuid>, DbError> {
    Ok(sqlx::query_scalar!(
        "SELECT refund_request_id FROM escalation_reviews WHERE status = 'pending' ORDER BY created_at"
    )
    .fetch_all(&db.0)
    .await?)
}

/// How the review call went, for `escalation_reviews`.
pub struct ReviewRun<'a> {
    pub model: &'a str,
    pub effort: &'a str,
    pub latency_ms: i32,
    pub prompt_tokens: Option<i32>,
    pub completion_tokens: Option<i32>,
}

/// Only a pending review is updated, so a duplicate run is a no-op.
pub async fn mark_review_drafted(
    db: &Db,
    refund_request_id: Uuid,
    draft: &Value,
    run: &ReviewRun<'_>,
) -> Result<(), DbError> {
    let mut tx = db.0.begin().await?;
    let updated = sqlx::query!(
        "UPDATE escalation_reviews
         SET status = 'drafted', draft = $2, model = $3, effort = $4, latency_ms = $5,
             prompt_tokens = $6, completion_tokens = $7, drafted_at = now()
         WHERE refund_request_id = $1 AND status = 'pending'",
        refund_request_id,
        draft,
        run.model,
        run.effort,
        run.latency_ms,
        run.prompt_tokens,
        run.completion_tokens,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 1 {
        sqlx::query!(
            "INSERT INTO request_events (refund_request_id, kind, actor_kind, payload)
             VALUES ($1, 'review_drafted', 'system', $2)",
            refund_request_id,
            json!({ "model": run.model }),
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn mark_review_failed(
    db: &Db,
    refund_request_id: Uuid,
    error: &str,
    run: &ReviewRun<'_>,
) -> Result<(), DbError> {
    let mut tx = db.0.begin().await?;
    let updated = sqlx::query!(
        "UPDATE escalation_reviews
         SET status = 'failed', error = $2, model = $3, effort = $4, latency_ms = $5
         WHERE refund_request_id = $1 AND status = 'pending'",
        refund_request_id,
        error,
        run.model,
        run.effort,
        run.latency_ms,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 1 {
        sqlx::query!(
            "INSERT INTO request_events (refund_request_id, kind, actor_kind, payload)
             VALUES ($1, 'review_failed', 'system', $2)",
            refund_request_id,
            json!({ "model": run.model, "error": error }),
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
