//! One pipeline run per conversation at a time. A session-level advisory lock
//! held on a dedicated pooled connection for the whole run, including the LLM
//! calls, so a transaction is not kept open while waiting on a model.

use sqlx::Postgres;
use sqlx::pool::PoolConnection;
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
