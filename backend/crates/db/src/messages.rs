//! Chat messages and their pre-scan signals. Bodies are plain text (ADR-023);
//! signal offsets are character offsets into the body.

use chrono::{DateTime, Utc};
use domain::prescan::{Detector, Signal};
use domain::types::{AssistantKind, MessageRole, SignalScope};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{Db, DbError, parse_enum};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Message {
    pub id: Uuid,
    #[serde(skip)]
    pub conversation_id: Uuid,
    pub seq: i32,
    pub role: MessageRole,
    pub assistant_kind: Option<AssistantKind>,
    pub body: String,
    #[serde(skip)]
    pub client_msg_id: Option<Uuid>,
    pub order_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SignalRow {
    pub message_id: Uuid,
    pub scope: SignalScope,
    pub detector: Detector,
    pub start: i32,
    pub end: i32,
    pub score: f32,
}

struct RawMessage {
    id: Uuid,
    conversation_id: Uuid,
    seq: i32,
    role: String,
    assistant_kind: Option<String>,
    body: String,
    client_msg_id: Option<Uuid>,
    order_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

impl TryFrom<RawMessage> for Message {
    type Error = DbError;

    fn try_from(r: RawMessage) -> Result<Self, DbError> {
        Ok(Message {
            id: r.id,
            conversation_id: r.conversation_id,
            seq: r.seq,
            role: parse_enum(&r.role)?,
            assistant_kind: r.assistant_kind.as_deref().map(parse_enum).transpose()?,
            body: r.body,
            client_msg_id: r.client_msg_id,
            order_id: r.order_id,
            created_at: r.created_at,
        })
    }
}

/// Allocates the next `seq` by bumping `conversations.last_seq`. The row lock
/// taken by that UPDATE orders concurrent inserts in the same conversation.
async fn next_seq(conn: &mut PgConnection, conversation_id: Uuid) -> Result<i32, DbError> {
    let seq = sqlx::query_scalar!(
        "UPDATE conversations SET last_seq = last_seq + 1, updated_at = now()
         WHERE id = $1 RETURNING last_seq",
        conversation_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(DbError::NotFound)?;
    Ok(seq)
}

/// A unique violation on `client_msg_id` means a retry raced this insert;
/// callers treat it as a duplicate.
pub async fn insert_customer_message(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    client_msg_id: Uuid,
    body: &str,
    order_id: Option<Uuid>,
) -> Result<Message, DbError> {
    let seq = next_seq(conn, conversation_id).await?;
    let r = sqlx::query_as!(
        RawMessage,
        "INSERT INTO messages (conversation_id, seq, role, body, client_msg_id, order_id)
         VALUES ($1, $2, 'customer', $3, $4, $5)
         RETURNING id, conversation_id, seq, role, assistant_kind, body, client_msg_id, order_id, created_at",
        conversation_id,
        seq,
        body,
        client_msg_id,
        order_id,
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        if crate::is_unique_violation(&e, "messages_client_msg_id_key") {
            DbError::Conflict("duplicate_client_msg_id")
        } else {
            e.into()
        }
    })?;
    r.try_into()
}

pub async fn insert_assistant_message(
    conn: &mut PgConnection,
    conversation_id: Uuid,
    kind: AssistantKind,
    body: &str,
) -> Result<Message, DbError> {
    let seq = next_seq(conn, conversation_id).await?;
    let r = sqlx::query_as!(
        RawMessage,
        "INSERT INTO messages (conversation_id, seq, role, assistant_kind, body)
         VALUES ($1, $2, 'assistant', $3, $4)
         RETURNING id, conversation_id, seq, role, assistant_kind, body, client_msg_id, order_id, created_at",
        conversation_id,
        seq,
        kind.as_str(),
        body,
    )
    .fetch_one(&mut *conn)
    .await?;
    r.try_into()
}

/// In `seq` order.
pub async fn list_messages(db: &Db, conversation_id: Uuid) -> Result<Vec<Message>, DbError> {
    sqlx::query_as!(
        RawMessage,
        "SELECT id, conversation_id, seq, role, assistant_kind, body, client_msg_id, order_id, created_at
         FROM messages WHERE conversation_id = $1 ORDER BY seq",
        conversation_id,
    )
    .fetch_all(&db.0)
    .await?
    .into_iter()
    .map(Message::try_from)
    .collect()
}

pub async fn find_message_by_client_id(
    db: &Db,
    client_msg_id: Uuid,
) -> Result<Option<Message>, DbError> {
    sqlx::query_as!(
        RawMessage,
        "SELECT id, conversation_id, seq, role, assistant_kind, body, client_msg_id, order_id, created_at
         FROM messages WHERE client_msg_id = $1",
        client_msg_id,
    )
    .fetch_optional(&db.0)
    .await?
    .map(Message::try_from)
    .transpose()
}

/// Clarifying questions already asked in this conversation.
pub async fn clarify_count(db: &Db, conversation_id: Uuid) -> Result<i64, DbError> {
    let n = sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM messages
           WHERE conversation_id = $1 AND assistant_kind = 'clarify'"#,
        conversation_id,
    )
    .fetch_one(&db.0)
    .await?;
    Ok(n)
}

/// Idempotent: re-scanning the same window inserts nothing new.
pub async fn insert_signals(
    conn: &mut PgConnection,
    message_id: Uuid,
    scope: SignalScope,
    signals: &[Signal],
) -> Result<(), DbError> {
    for s in signals {
        // Bodies are at most 4000 characters, so offsets always fit.
        let start = i32::try_from(s.start).unwrap_or(i32::MAX);
        let end = i32::try_from(s.end).unwrap_or(i32::MAX);
        sqlx::query!(
            "INSERT INTO message_signals (message_id, scope, detector, start_char, end_char, score)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT DO NOTHING",
            message_id,
            scope.as_str(),
            s.detector.as_str(),
            start,
            end,
            s.score,
        )
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Every signal on every message of the conversation, in message order.
pub async fn list_signals_for_conversation(
    db: &Db,
    conversation_id: Uuid,
) -> Result<Vec<SignalRow>, DbError> {
    let rows = sqlx::query!(
        "SELECT s.message_id, s.scope, s.detector, s.start_char, s.end_char, s.score
         FROM message_signals s
         JOIN messages m ON m.id = s.message_id
         WHERE m.conversation_id = $1
         ORDER BY m.seq, s.start_char, s.scope, s.detector",
        conversation_id,
    )
    .fetch_all(&db.0)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(SignalRow {
                message_id: r.message_id,
                scope: parse_enum(&r.scope)?,
                detector: parse_enum(&r.detector)?,
                start: r.start_char,
                end: r.end_char,
                score: r.score,
            })
        })
        .collect()
}
