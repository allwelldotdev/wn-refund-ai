//! Throwaway databases on the same server, for the live eval tools: each run
//! gets freshly migrated and seeded data and never reads or changes the demo
//! database.

use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, ConnectOptions, Connection, Executor};

use crate::{Db, DbError};

pub struct Scratch {
    pub db: Db,
    pub name: String,
    server: PgConnectOptions,
}

/// Creates an empty database next to the one `database_url` names. The name is
/// generated here, never taken from input, so formatting it into SQL is safe.
pub async fn create(database_url: &str) -> Result<Scratch, DbError> {
    let server = PgConnectOptions::from_str(database_url)?;
    let name = format!("refund_eval_{}", Utc::now().timestamp_millis());
    let mut conn = server.connect().await?;
    conn.execute(AssertSqlSafe(format!(r#"CREATE DATABASE "{name}""#)))
        .await?;
    conn.close().await?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(server.clone().database(&name))
        .await?;
    Ok(Scratch {
        db: Db(pool),
        name,
        server,
    })
}

impl Scratch {
    pub async fn drop_database(self) -> Result<(), DbError> {
        self.db.0.close().await;
        let mut conn = self.server.connect().await?;
        conn.execute(AssertSqlSafe(format!(
            r#"DROP DATABASE "{}" WITH (FORCE)"#,
            self.name
        )))
        .await?;
        conn.close().await?;
        Ok(())
    }
}
