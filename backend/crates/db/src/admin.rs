//! Admin views of refund requests: the queue, the case file, the raw audit
//! rows, and the final human decision on escalations.

use chrono::{DateTime, Utc};
use domain::types::{AssistantKind, Flag, MessageRole, ReasonCategory, RequestState, SignalScope};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::messages::SignalRow;
use crate::{Db, DbError, is_unique_violation, parse_enum};

pub struct ListFilter {
    pub state: Option<RequestState>,
    /// Matched case-insensitively against request ref, customer name and
    /// email, and order ref.
    pub q: Option<String>,
    /// Only requests created at or after this instant.
    pub since: Option<DateTime<Utc>>,
    /// Each group is a set of flag names; a request matches when its decision
    /// audit carries at least one flag from every group.
    pub flag_groups: Vec<Vec<Flag>>,
    /// Only requests a customer disputed.
    pub disputed: bool,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListItem {
    #[serde(rename = "ref")]
    pub request_ref: String,
    pub state: RequestState,
    pub customer_name: String,
    pub customer_email: String,
    pub order_ref: Option<String>,
    pub item_name: Option<String>,
    pub amount_cents: Option<i64>,
    pub reason_category: Option<ReasonCategory>,
    pub flags: Value,
    pub review_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub disputed_at: Option<DateTime<Utc>>,
    /// Customer messages after the decision that no admin has read.
    pub unread_from_customer: i64,
    /// When the customer last wrote after the decision, while no admin has
    /// replied since.
    pub customer_replied_at: Option<DateTime<Utc>>,
}

/// Counts for the admin overview. `created` covers requests created at or
/// after `since`, by their current state; the open-escalation figures cover
/// every request still waiting for an admin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub since: DateTime<Utc>,
    pub created: CreatedCounts,
    pub open_escalations: i64,
    pub oldest_open_escalation_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CreatedCounts {
    pub total: i64,
    pub approved: i64,
    pub denied: i64,
    pub escalated: i64,
    pub resolved_approved: i64,
    pub resolved_denied: i64,
}

pub async fn stats(db: &Db, since: DateTime<Utc>) -> Result<Stats, DbError> {
    let r = sqlx::query!(
        r#"SELECT count(*) FILTER (WHERE created_at >= $1) AS "total!",
                  count(*) FILTER (WHERE created_at >= $1 AND state = 'approved') AS "approved!",
                  count(*) FILTER (WHERE created_at >= $1 AND state = 'denied') AS "denied!",
                  count(*) FILTER (WHERE created_at >= $1 AND state = 'escalated') AS "escalated!",
                  count(*) FILTER (WHERE created_at >= $1 AND state = 'resolved_approved') AS "resolved_approved!",
                  count(*) FILTER (WHERE created_at >= $1 AND state = 'resolved_denied') AS "resolved_denied!",
                  count(*) FILTER (WHERE state = 'escalated') AS "open_escalations!",
                  min(created_at) FILTER (WHERE state = 'escalated') AS oldest_open_escalation_at
           FROM refund_requests"#,
        since,
    )
    .fetch_one(&db.0)
    .await?;
    Ok(Stats {
        since,
        created: CreatedCounts {
            total: r.total,
            approved: r.approved,
            denied: r.denied,
            escalated: r.escalated,
            resolved_approved: r.resolved_approved,
            resolved_denied: r.resolved_denied,
        },
        open_escalations: r.open_escalations,
        oldest_open_escalation_at: r.oldest_open_escalation_at,
    })
}

/// `%`, `_` and `\` in the search text match literally.
fn like_pattern(q: &str) -> String {
    let escaped = q
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

/// Newest first, with the total for paging.
pub async fn list_requests(db: &Db, f: &ListFilter) -> Result<(Vec<ListItem>, i64), DbError> {
    let state = f.state.map(|s| s.as_str());
    let pattern =
        f.q.as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(like_pattern);
    // One comma-joined string per group keeps the query static.
    let groups: Vec<String> = f
        .flag_groups
        .iter()
        .map(|g| {
            g.iter()
                .map(|flag| flag.as_str())
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect();
    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "total!"
           FROM refund_requests r
           JOIN customers c ON c.id = r.customer_id
           LEFT JOIN orders o ON o.id = r.order_id
           LEFT JOIN decision_audit a ON a.refund_request_id = r.id
           WHERE ($1::text IS NULL OR r.state = $1)
             AND ($2::text IS NULL OR r.ref ILIKE $2 OR c.name ILIKE $2
                  OR c.email ILIKE $2 OR o.ref ILIKE $2)
             AND ($3::timestamptz IS NULL OR r.created_at >= $3)
             AND NOT EXISTS (
                   SELECT 1 FROM unnest($4::text[]) AS g(grp)
                   WHERE NOT (coalesce(a.flags, '[]'::jsonb) ?| string_to_array(g.grp, ',')))
             AND (NOT $5 OR r.disputed_at IS NOT NULL)"#,
        state,
        pattern,
        f.since,
        &groups,
        f.disputed,
    )
    .fetch_one(&db.0)
    .await?;
    let rows = sqlx::query!(
        r#"SELECT r.ref, r.state, c.name, c.email, o.ref AS "order_ref?", i.name AS "item_name?",
                  r.amount_cents, r.reason_category, a.flags AS "flags?", v.status AS "review_status?",
                  r.created_at, r.resolved_at, r.disputed_at,
                  (SELECT count(*) FROM messages m
                   WHERE m.conversation_id = r.conversation_id AND m.role = 'customer'
                     AND m.seq > GREATEST(cv.admin_read_seq, coalesce(a.evaluated_through_seq, 0)))
                    AS "unread_from_customer!",
                  (SELECT max(m.created_at) FROM messages m
                   WHERE m.conversation_id = r.conversation_id AND m.role = 'customer'
                     AND m.seq > coalesce(a.evaluated_through_seq, 0)
                     AND m.seq > coalesce((SELECT max(x.seq) FROM messages x
                                           WHERE x.conversation_id = r.conversation_id
                                             AND x.role = 'admin'), 0))
                    AS "customer_replied_at?"
           FROM refund_requests r
           JOIN customers c ON c.id = r.customer_id
           JOIN conversations cv ON cv.id = r.conversation_id
           LEFT JOIN orders o ON o.id = r.order_id
           LEFT JOIN order_items i ON i.id = r.order_item_id
           LEFT JOIN decision_audit a ON a.refund_request_id = r.id
           LEFT JOIN escalation_reviews v ON v.refund_request_id = r.id
           WHERE ($1::text IS NULL OR r.state = $1)
             AND ($2::text IS NULL OR r.ref ILIKE $2 OR c.name ILIKE $2
                  OR c.email ILIKE $2 OR o.ref ILIKE $2)
             AND ($3::timestamptz IS NULL OR r.created_at >= $3)
             AND NOT EXISTS (
                   SELECT 1 FROM unnest($4::text[]) AS g(grp)
                   WHERE NOT (coalesce(a.flags, '[]'::jsonb) ?| string_to_array(g.grp, ',')))
             AND (NOT $5 OR r.disputed_at IS NOT NULL)
           ORDER BY r.created_at DESC, r.id
           LIMIT $6 OFFSET $7"#,
        state,
        pattern,
        f.since,
        &groups,
        f.disputed,
        f.limit,
        f.offset,
    )
    .fetch_all(&db.0)
    .await?;
    let items = rows
        .into_iter()
        .map(|r| {
            Ok(ListItem {
                request_ref: r.r#ref,
                state: parse_enum(&r.state)?,
                customer_name: r.name,
                customer_email: r.email,
                order_ref: r.order_ref,
                item_name: r.item_name,
                amount_cents: r.amount_cents,
                reason_category: r.reason_category.as_deref().map(parse_enum).transpose()?,
                flags: r.flags.unwrap_or_else(|| json!([])),
                review_status: r.review_status,
                created_at: r.created_at,
                resolved_at: r.resolved_at,
                disputed_at: r.disputed_at,
                unread_from_customer: r.unread_from_customer,
                customer_replied_at: r.customer_replied_at,
            })
        })
        .collect::<Result<_, DbError>>()?;
    Ok((items, total))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RequestDetail {
    pub request: RequestInfo,
    pub customer: CustomerInfo,
    pub order: Option<OrderInfo>,
    pub timeline: Vec<TimelineEvent>,
    pub messages: Vec<DetailMessage>,
    pub audit: Option<AuditInfo>,
    pub review: Option<ReviewInfo>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RequestInfo {
    #[serde(rename = "ref")]
    pub request_ref: String,
    pub state: RequestState,
    pub reason_category: Option<ReasonCategory>,
    pub amount_cents: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub disputed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CustomerInfo {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub scenario: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OrderInfo {
    #[serde(rename = "ref")]
    pub order_ref: String,
    pub placed_at: DateTime<Utc>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub item: ItemInfo,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ItemInfo {
    pub name: String,
    pub category: String,
    pub amount_cents: i64,
    pub final_sale: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TimelineEvent {
    pub kind: String,
    pub actor_kind: String,
    pub actor_name: Option<String>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

/// Whether the decision read this message (ADR-022).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageTag {
    UsedInDecision,
    AfterDecision,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DetailMessage {
    pub id: Uuid,
    pub seq: i32,
    pub role: MessageRole,
    pub assistant_kind: Option<AssistantKind>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    /// The admin who wrote an admin message.
    pub author_name: Option<String>,
    pub tag: Option<MessageTag>,
    pub signals: Vec<SignalView>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SignalView {
    pub scope: SignalScope,
    pub detector: String,
    pub start: i32,
    pub end: i32,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuditInfo {
    pub verdict: String,
    pub flags: Value,
    pub rule_trace: Value,
    pub extracted: Option<Value>,
    pub facts: Value,
    pub stages: Value,
    pub evaluated_through_seq: i32,
    pub policy_version: PolicyRef,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PolicyRef {
    pub id: Uuid,
    pub version: i32,
    pub content_hash: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReviewInfo {
    pub status: String,
    pub draft: Option<Value>,
    pub error: Option<String>,
    pub model: Option<String>,
    pub latency_ms: Option<i32>,
    pub resolution: Option<String>,
    pub resolution_note: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
}

pub async fn get_request_detail(
    db: &Db,
    request_ref: &str,
) -> Result<Option<RequestDetail>, DbError> {
    let Some(r) = sqlx::query!(
        r#"SELECT r.id, r.ref, r.state, r.reason_category, r.amount_cents, r.created_at, r.resolved_at,
                  r.disputed_at, r.conversation_id, c.id AS customer_id, c.name, c.email, c.scenario,
                  o.ref AS "order_ref?", o.placed_at AS "placed_at?", o.delivered_at,
                  i.name AS "item_name?", i.category AS "category?",
                  i.amount_cents AS "item_amount_cents?", i.final_sale AS "final_sale?"
           FROM refund_requests r
           JOIN customers c ON c.id = r.customer_id
           LEFT JOIN orders o ON o.id = r.order_id
           LEFT JOIN order_items i ON i.id = r.order_item_id
           WHERE r.ref = $1"#,
        request_ref,
    )
    .fetch_optional(&db.0)
    .await?
    else {
        return Ok(None);
    };

    let order = match (
        r.order_ref,
        r.placed_at,
        r.item_name,
        r.category,
        r.item_amount_cents,
        r.final_sale,
    ) {
        (Some(order_ref), Some(placed_at), Some(name), Some(category), Some(amount), Some(fs)) => {
            Some(OrderInfo {
                order_ref,
                placed_at,
                delivered_at: r.delivered_at,
                item: ItemInfo {
                    name,
                    category,
                    amount_cents: amount,
                    final_sale: fs,
                },
            })
        }
        _ => None,
    };

    let timeline = sqlx::query!(
        r#"SELECT e.kind, e.actor_kind, a.name AS "actor_name?", e.payload, e.created_at
           FROM request_events e
           LEFT JOIN admins a ON a.id = e.actor_admin_id
           WHERE e.refund_request_id = $1
           ORDER BY e.created_at, e.id"#,
        r.id,
    )
    .fetch_all(&db.0)
    .await?
    .into_iter()
    .map(|e| TimelineEvent {
        kind: e.kind,
        actor_kind: e.actor_kind,
        actor_name: e.actor_name,
        payload: e.payload,
        created_at: e.created_at,
    })
    .collect();

    let audit = sqlx::query!(
        "SELECT a.verdict, a.flags, a.rule_trace, a.extracted, a.facts, a.stages,
                a.evaluated_through_seq, a.policy_version_id, p.version, a.content_hash
         FROM decision_audit a
         JOIN policy_versions p ON p.id = a.policy_version_id
         WHERE a.refund_request_id = $1",
        r.id,
    )
    .fetch_optional(&db.0)
    .await?
    .map(|a| AuditInfo {
        verdict: a.verdict,
        flags: a.flags,
        rule_trace: a.rule_trace,
        extracted: a.extracted,
        facts: a.facts,
        stages: a.stages,
        evaluated_through_seq: a.evaluated_through_seq,
        policy_version: PolicyRef {
            id: a.policy_version_id,
            version: a.version,
            content_hash: a.content_hash,
        },
    });

    let review = sqlx::query!(
        r#"SELECT v.status, v.draft, v.error, v.model, v.latency_ms, v.resolution,
                  v.resolution_note, a.name AS "resolved_by?", v.resolved_at
           FROM escalation_reviews v
           LEFT JOIN admins a ON a.id = v.resolved_by_admin_id
           WHERE v.refund_request_id = $1"#,
        r.id,
    )
    .fetch_optional(&db.0)
    .await?
    .map(|v| ReviewInfo {
        status: v.status,
        draft: v.draft,
        error: v.error,
        model: v.model,
        latency_ms: v.latency_ms,
        resolution: v.resolution,
        resolution_note: v.resolution_note,
        resolved_by: v.resolved_by,
        resolved_at: v.resolved_at,
    });

    let evaluated = audit.as_ref().map(|a| a.evaluated_through_seq);
    let signals = crate::messages::list_signals_for_conversation(db, r.conversation_id).await?;
    let messages = crate::messages::list_messages(db, r.conversation_id)
        .await?
        .into_iter()
        .map(|m| {
            let tag = match (m.role, evaluated) {
                (MessageRole::Customer, Some(through)) if m.seq <= through => {
                    Some(MessageTag::UsedInDecision)
                }
                (MessageRole::Customer, Some(_)) => Some(MessageTag::AfterDecision),
                _ => None,
            };
            DetailMessage {
                signals: signals
                    .iter()
                    .filter(|s| s.message_id == m.id)
                    .map(signal_view)
                    .collect(),
                id: m.id,
                seq: m.seq,
                role: m.role,
                assistant_kind: m.assistant_kind,
                body: m.body,
                created_at: m.created_at,
                author_name: m.author_name,
                tag,
            }
        })
        .collect();

    Ok(Some(RequestDetail {
        request: RequestInfo {
            request_ref: r.r#ref,
            state: parse_enum(&r.state)?,
            reason_category: r.reason_category.as_deref().map(parse_enum).transpose()?,
            amount_cents: r.amount_cents,
            created_at: r.created_at,
            resolved_at: r.resolved_at,
            disputed_at: r.disputed_at,
        },
        customer: CustomerInfo {
            id: r.customer_id,
            name: r.name,
            email: r.email,
            scenario: r.scenario,
        },
        order,
        timeline,
        messages,
        audit,
        review,
    }))
}

fn signal_view(s: &SignalRow) -> SignalView {
    SignalView {
        scope: s.scope,
        detector: s.detector.as_str().to_owned(),
        start: s.start,
        end: s.end,
        score: s.score,
    }
}

/// The stored rows as they are, for the "raw audit" view.
pub async fn get_request_audit_raw(db: &Db, request_ref: &str) -> Result<Option<Value>, DbError> {
    let row = sqlx::query!(
        r#"SELECT to_jsonb(r) AS "request!",
                  (SELECT to_jsonb(a) FROM decision_audit a WHERE a.refund_request_id = r.id) AS decision_audit,
                  (SELECT to_jsonb(v) FROM escalation_reviews v WHERE v.refund_request_id = r.id) AS escalation_review,
                  (SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY e.created_at, e.id), '[]'::jsonb)
                   FROM request_events e WHERE e.refund_request_id = r.id) AS "events!"
           FROM refund_requests r WHERE r.ref = $1"#,
        request_ref,
    )
    .fetch_optional(&db.0)
    .await?;
    Ok(row.map(|r| {
        json!({
            "request": r.request,
            "decision_audit": r.decision_audit,
            "escalation_review": r.escalation_review,
            "events": r.events,
        })
    }))
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resolution {
    Approved,
    Denied,
}

impl Resolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Resolution::Approved => "approved",
            Resolution::Denied => "denied",
        }
    }

    pub fn state(self) -> RequestState {
        match self {
            Resolution::Approved => RequestState::ResolvedApproved,
            Resolution::Denied => RequestState::ResolvedDenied,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Resolved {
    #[serde(rename = "ref")]
    pub request_ref: String,
    pub state: RequestState,
    pub resolved_at: DateTime<Utc>,
}

/// What the resolution notice is written from.
pub struct NoticeFacts {
    pub state: RequestState,
    pub customer_name: String,
    pub item_name: Option<String>,
    pub order_ref: Option<String>,
    pub amount_cents: Option<i64>,
}

pub async fn notice_facts(db: &Db, request_ref: &str) -> Result<Option<NoticeFacts>, DbError> {
    let r = sqlx::query!(
        r#"SELECT r.state AS "state!", c.name AS "customer_name!", i.name AS "item_name?",
                  o.ref AS "order_ref?", r.amount_cents
           FROM refund_requests r
           JOIN customers c ON c.id = r.customer_id
           LEFT JOIN orders o ON o.id = r.order_id
           LEFT JOIN order_items i ON i.id = r.order_item_id
           WHERE r.ref = $1"#,
        request_ref,
    )
    .fetch_optional(&db.0)
    .await?;
    r.map(|r| {
        Ok(NoticeFacts {
            state: parse_enum(&r.state)?,
            customer_name: r.customer_name,
            item_name: r.item_name,
            order_ref: r.order_ref,
            amount_cents: r.amount_cents,
        })
    })
    .transpose()
}

/// The admin's decision, and what the customer is told about it.
pub struct Resolve<'a> {
    pub resolution: Resolution,
    /// Kept for the audit; never shown to the customer as written.
    pub note: &'a str,
    /// Posted to the chat as a message from the support team.
    pub message: &'a str,
    /// Posted to the chat as a system note.
    pub summary: &'a str,
}

/// Escalated → resolved_*, with the admin's note on the review, a `resolved`
/// event, and the message and summary posted to the customer's chat, all in
/// one transaction. `NotFound`, `Conflict("not_escalated")`, or
/// `Conflict("duplicate_active_refund")` when approving an item that already
/// has an approved refund.
pub async fn resolve_request(
    db: &Db,
    request_ref: &str,
    admin_id: Uuid,
    r: &Resolve<'_>,
) -> Result<Resolved, DbError> {
    let (resolution, note) = (r.resolution, r.note);
    let state = resolution.state();
    let mut tx = db.0.begin().await?;
    // The customer's ledger (`lock::lock_customer_xact`): every write that
    // decides a request for this customer holds it.
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended('customer:' || customer_id::text, 0))
         FROM refund_requests WHERE ref = $1",
        request_ref,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let updated = sqlx::query!(
        "UPDATE refund_requests SET state = $2, resolved_at = now()
         WHERE ref = $1 AND state = 'escalated'
         RETURNING id, conversation_id, resolved_at AS \"resolved_at!\"",
        request_ref,
        state.as_str(),
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        if is_unique_violation(&e, "refund_requests_one_active_per_item") {
            DbError::Conflict("duplicate_active_refund")
        } else {
            e.into()
        }
    })?;
    let Some(updated) = updated else {
        let exists = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM refund_requests WHERE ref = $1) AS "exists!""#,
            request_ref,
        )
        .fetch_one(&mut *tx)
        .await?;
        return Err(if exists {
            DbError::Conflict("not_escalated")
        } else {
            DbError::NotFound
        });
    };
    sqlx::query!(
        "UPDATE escalation_reviews
         SET resolution = $2, resolution_note = $3, resolved_by_admin_id = $4, resolved_at = $5
         WHERE refund_request_id = $1",
        updated.id,
        resolution.as_str(),
        note,
        admin_id,
        updated.resolved_at,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO request_events (refund_request_id, kind, actor_kind, actor_admin_id, payload, created_at)
         VALUES ($1, 'resolved', 'admin', $2, $3, $4)",
        updated.id,
        admin_id,
        json!({ "resolution": resolution.as_str(), "note": note }),
        updated.resolved_at,
    )
    .execute(&mut *tx)
    .await?;
    let chat = updated.conversation_id;
    crate::messages::insert_note(&mut tx, chat, MessageRole::Admin, r.message, Some(admin_id))
        .await?;
    crate::messages::insert_note(&mut tx, chat, MessageRole::System, r.summary, None).await?;
    tx.commit().await?;
    Ok(Resolved {
        request_ref: request_ref.to_owned(),
        state,
        resolved_at: updated.resolved_at,
    })
}

/// An admin's message to the customer on an escalated request, stored exactly
/// as written. `NotFound`, or `Conflict("not_escalated")` once decided.
pub async fn post_admin_message(
    db: &Db,
    request_ref: &str,
    admin_id: Uuid,
    body: &str,
) -> Result<crate::messages::Message, DbError> {
    let mut tx = db.0.begin().await?;
    // The row lock keeps a resolve from landing between the check and the insert.
    let r = sqlx::query!(
        "SELECT conversation_id, state FROM refund_requests WHERE ref = $1 FOR UPDATE",
        request_ref,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(DbError::NotFound)?;
    if r.state != RequestState::Escalated.as_str() {
        return Err(DbError::Conflict("not_escalated"));
    }
    let message = crate::messages::insert_note(
        &mut tx,
        r.conversation_id,
        MessageRole::Admin,
        body,
        Some(admin_id),
    )
    .await?;
    tx.commit().await?;
    Ok(message)
}

/// Moves the admins' shared read marker on the request's conversation up to
/// `seq` (never back, never past the last message).
pub async fn mark_admin_read(db: &Db, request_ref: &str, seq: i32) -> Result<(), DbError> {
    let done = sqlx::query!(
        "UPDATE conversations c
         SET admin_read_seq = GREATEST(c.admin_read_seq, LEAST($2, c.last_seq))
         FROM refund_requests r
         WHERE r.conversation_id = c.id AND r.ref = $1",
        request_ref,
        seq,
    )
    .execute(&db.0)
    .await?;
    if done.rows_affected() == 0 {
        return Err(DbError::NotFound);
    }
    Ok(())
}
