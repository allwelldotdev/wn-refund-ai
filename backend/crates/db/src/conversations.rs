//! Conversations and the per-conversation request summary shown to customers.

use chrono::{DateTime, Utc};
use domain::types::RequestState;
use serde::Serialize;
use uuid::Uuid;

use crate::{Db, DbError, parse_enum};

/// Characters of the first customer message shown in the conversation list.
const PREVIEW_CHARS: usize = 80;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Conversation {
    pub id: Uuid,
    #[serde(skip)]
    pub customer_id: Uuid,
    pub last_seq: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The one refund request a conversation can produce (ADR-021).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RequestSummary {
    pub id: Uuid,
    #[serde(rename = "ref")]
    pub request_ref: String,
    pub state: RequestState,
    pub order_ref: Option<String>,
    pub item_name: Option<String>,
    pub amount_cents: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub disputed_at: Option<DateTime<Utc>>,
    /// An automatic denial, not yet disputed, while disputes are allowed.
    pub can_dispute: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConversationSummary {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_seq: i32,
    pub preview: Option<String>,
    pub request: Option<RequestSummary>,
    /// Admin messages the customer has not seen yet.
    pub unread_count: i64,
    /// The latest admin message: when, and who wrote it.
    pub last_reply_at: Option<DateTime<Utc>>,
    pub last_reply_by: Option<String>,
}

pub async fn create_conversation(db: &Db, customer_id: Uuid) -> Result<Conversation, DbError> {
    let r = sqlx::query!(
        "INSERT INTO conversations (customer_id) VALUES ($1)
         RETURNING id, customer_id, last_seq, created_at, updated_at",
        customer_id,
    )
    .fetch_one(&db.0)
    .await?;
    Ok(Conversation {
        id: r.id,
        customer_id: r.customer_id,
        last_seq: r.last_seq,
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
}

/// `None` both when the conversation does not exist and when it belongs to
/// someone else, so callers cannot probe other customers' ids.
pub async fn get_conversation_owned(
    db: &Db,
    id: Uuid,
    customer_id: Uuid,
) -> Result<Option<Conversation>, DbError> {
    let r = sqlx::query!(
        "SELECT id, customer_id, last_seq, created_at, updated_at
         FROM conversations WHERE id = $1 AND customer_id = $2",
        id,
        customer_id,
    )
    .fetch_optional(&db.0)
    .await?;
    Ok(r.map(|r| Conversation {
        id: r.id,
        customer_id: r.customer_id,
        last_seq: r.last_seq,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

pub async fn last_seq(db: &Db, conversation_id: Uuid) -> Result<i32, DbError> {
    let seq = sqlx::query_scalar!(
        "SELECT last_seq FROM conversations WHERE id = $1",
        conversation_id,
    )
    .fetch_optional(&db.0)
    .await?
    .ok_or(DbError::NotFound)?;
    Ok(seq)
}

/// Most recently active first.
pub async fn list_conversations(
    db: &Db,
    customer_id: Uuid,
) -> Result<Vec<ConversationSummary>, DbError> {
    let rows = sqlx::query!(
        r#"SELECT c.id, c.created_at, c.updated_at, c.last_seq,
                  (SELECT left(m.body, $2) FROM messages m
                   WHERE m.conversation_id = c.id AND m.role = 'customer'
                   ORDER BY m.seq LIMIT 1) AS preview,
                  r.id AS "request_id?", r.ref AS "request_ref?", r.state AS "state?",
                  o.ref AS "order_ref?", i.name AS "item_name?", r.amount_cents AS "amount_cents?",
                  r.created_at AS "request_created_at?", r.resolved_at AS "resolved_at?",
                  r.disputed_at AS "disputed_at?",
                  (r.state = 'denied' AND r.disputed_at IS NULL
                   AND (SELECT allow_disputes FROM app_settings)) AS "can_dispute?",
                  (SELECT count(*) FROM messages m
                   WHERE m.conversation_id = c.id AND m.role = 'admin'
                     AND m.seq > c.customer_read_seq) AS "unread_count!",
                  (SELECT max(m.created_at) FROM messages m
                   WHERE m.conversation_id = c.id AND m.role = 'admin') AS "last_reply_at?",
                  (SELECT (SELECT a.name FROM admins a WHERE a.id = m.author_admin_id)
                   FROM messages m WHERE m.conversation_id = c.id AND m.role = 'admin'
                   ORDER BY m.seq DESC LIMIT 1) AS "last_reply_by?"
           FROM conversations c
           LEFT JOIN refund_requests r ON r.conversation_id = c.id
           LEFT JOIN orders o ON o.id = r.order_id
           LEFT JOIN order_items i ON i.id = r.order_item_id
           WHERE c.customer_id = $1
           ORDER BY c.updated_at DESC, c.id"#,
        customer_id,
        PREVIEW_CHARS as i32,
    )
    .fetch_all(&db.0)
    .await?;
    rows.into_iter()
        .map(|r| {
            let request = match (r.request_id, r.request_ref, r.state, r.request_created_at) {
                (Some(id), Some(request_ref), Some(state), Some(created_at)) => {
                    Some(RequestSummary {
                        id,
                        request_ref,
                        state: parse_enum(&state)?,
                        order_ref: r.order_ref,
                        item_name: r.item_name,
                        amount_cents: r.amount_cents,
                        created_at,
                        resolved_at: r.resolved_at,
                        disputed_at: r.disputed_at,
                        can_dispute: r.can_dispute.unwrap_or(false),
                    })
                }
                _ => None,
            };
            Ok(ConversationSummary {
                id: r.id,
                created_at: r.created_at,
                updated_at: r.updated_at,
                last_seq: r.last_seq,
                preview: r.preview,
                request,
                unread_count: r.unread_count,
                last_reply_at: r.last_reply_at,
                last_reply_by: r.last_reply_by,
            })
        })
        .collect()
}

/// Moves the customer's read marker up to `seq` (never back, never past the
/// last message). `NotFound` when the conversation is not theirs.
pub async fn mark_customer_read(
    db: &Db,
    id: Uuid,
    customer_id: Uuid,
    seq: i32,
) -> Result<(), DbError> {
    let done = sqlx::query!(
        "UPDATE conversations
         SET customer_read_seq = GREATEST(customer_read_seq, LEAST($3, last_seq))
         WHERE id = $1 AND customer_id = $2",
        id,
        customer_id,
        seq,
    )
    .execute(&db.0)
    .await?;
    if done.rows_affected() == 0 {
        return Err(DbError::NotFound);
    }
    Ok(())
}

pub async fn find_request_summary(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Option<RequestSummary>, DbError> {
    let r = sqlx::query!(
        // `!`: nullability is otherwise inferred from the live query plan,
        // which can put `r` on the nullable side of the joins.
        r#"SELECT r.id AS "id!", r.ref AS "ref!", r.state AS "state!",
                  o.ref AS "order_ref?", i.name AS "item_name?",
                  r.amount_cents, r.created_at AS "created_at!", r.resolved_at, r.disputed_at,
                  (r.state = 'denied' AND r.disputed_at IS NULL
                   AND (SELECT allow_disputes FROM app_settings)) AS "can_dispute!"
           FROM refund_requests r
           LEFT JOIN orders o ON o.id = r.order_id
           LEFT JOIN order_items i ON i.id = r.order_item_id
           WHERE r.conversation_id = $1"#,
        conversation_id,
    )
    .fetch_optional(&db.0)
    .await?;
    r.map(|r| {
        Ok(RequestSummary {
            id: r.id,
            request_ref: r.r#ref,
            state: parse_enum(&r.state)?,
            order_ref: r.order_ref,
            item_name: r.item_name,
            amount_cents: r.amount_cents,
            created_at: r.created_at,
            resolved_at: r.resolved_at,
            disputed_at: r.disputed_at,
            can_dispute: r.can_dispute,
        })
    })
    .transpose()
}
