//! Advisory locks.
//!
//! One pipeline run per conversation at a time: a session-level lock held on
//! a dedicated pooled connection for the whole run, including the LLM calls,
//! so a transaction is not kept open while waiting on a model.
//!
//! The customer's ledger: a transaction-level lock, in its own key space,
//! taken by every write that decides a request for that customer, so two of
//! their chats (or a chat and an admin) never decide on the same stale facts.

use sqlx::pool::PoolConnection;
use sqlx::{PgConnection, Postgres};
use uuid::Uuid;

use crate::{Db, DbError};

pub struct ConversationLock {
    conn: Option<PoolConnection<Postgres>>,
    key: String,
}

pub async fn lock_conversation(
    db: &Db,
    conversation_id: Uuid,
) -> Result<ConversationLock, DbError> {
    let mut conn = db.0.acquire().await?;
    let key = conversation_id.to_string();
    sqlx::query!("SELECT pg_advisory_lock(hashtextextended($1, 0))", key)
        .execute(&mut *conn)
        .await?;
    Ok(ConversationLock {
        conn: Some(conn),
        key,
    })
}

impl ConversationLock {
    pub async fn release(mut self) -> Result<(), DbError> {
        let Some(mut conn) = self.conn.take() else {
            return Ok(());
        };
        let unlocked = sqlx::query_scalar!(
            r#"SELECT pg_advisory_unlock(hashtextextended($1, 0)) AS "unlocked!""#,
            self.key
        )
        .fetch_one(&mut *conn)
        .await;
        match unlocked {
            Ok(true) => Ok(()),
            // Never return a connection that may still hold the lock.
            Ok(false) => {
                drop(conn.detach());
                Err(DbError::Corrupt("conversation lock was not held".into()))
            }
            Err(e) => {
                drop(conn.detach());
                Err(e.into())
            }
        }
    }
}

impl Drop for ConversationLock {
    /// An unreleased lock (panic, early return) closes its connection instead
    /// of returning it to the pool; Postgres frees session locks on disconnect.
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            drop(conn.detach());
        }
    }
}

/// Held until the surrounding transaction ends. Keyed `customer:<id>`, so it
/// never collides with a conversation lock.
pub async fn lock_customer_xact(conn: &mut PgConnection, customer_id: Uuid) -> Result<(), DbError> {
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtextextended('customer:' || $1::uuid::text, 0))",
        customer_id
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}
