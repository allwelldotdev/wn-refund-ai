//! Postgres access for the refund system: pool, migrations, queries and the
//! idempotent demo seed. Queries use SQLx compile-time checking; the offline
//! metadata in `backend/.sqlx` lets Docker builds compile without a database.

pub mod auth;
pub mod conversations;
pub mod lock;
pub mod messages;
pub mod orders;
pub mod policy;
pub mod refunds;
pub mod seed;

use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Db(pub PgPool);

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("default policy is invalid: {0:?}")]
    InvalidPolicy(Vec<domain::policy::FieldError>),
    #[error("password hashing failed: {0}")]
    PasswordHash(String),
    #[error("not found")]
    NotFound,
    /// A business rule enforced by the database, e.g. `duplicate_active_refund`.
    #[error("conflict: {0}")]
    Conflict(&'static str),
    /// A policy edit was based on a version that is no longer the latest.
    #[error("stale base: latest policy is version {latest_version}")]
    StaleBase {
        latest_id: Uuid,
        latest_version: i32,
    },
    /// A policy edit that would not change the rules.
    #[error("no change")]
    NoOp,
    /// A stored value the code cannot interpret (a data-integrity bug).
    #[error("corrupt row: {0}")]
    Corrupt(String),
}

/// Decodes a `text` column holding one of the domain enums. The CHECK
/// constraints make a failure here a bug, not user error.
pub(crate) fn parse_enum<T>(value: &str) -> Result<T, DbError>
where
    T: std::str::FromStr<Err = domain::types::UnknownVariant>,
{
    value
        .parse()
        .map_err(|e: domain::types::UnknownVariant| DbError::Corrupt(e.to_string()))
}

/// Decodes a `jsonb` column into its domain type.
pub(crate) fn from_json<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Result<T, DbError> {
    serde_json::from_value(value).map_err(|e| DbError::Corrupt(e.to_string()))
}

pub(crate) fn is_unique_violation(err: &sqlx::Error, constraint: &str) -> bool {
    err.as_database_error()
        .is_some_and(|e| e.is_unique_violation() && e.constraint() == Some(constraint))
}

/// Each in-flight message pipeline holds one connection for its conversation
/// lock, so the pool is sized above the expected concurrency.
pub async fn connect(database_url: &str) -> Result<Db, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await?;
    Ok(Db(pool))
}

pub async fn migrate(db: &Db) -> Result<(), DbError> {
    sqlx::migrate!("../../migrations").run(&db.0).await?;
    Ok(())
}

/// Cheap liveness probe used by the health endpoint.
pub async fn ping(db: &Db) -> Result<(), DbError> {
    sqlx::query_scalar!("SELECT 1 AS \"one!\"")
        .fetch_one(&db.0)
        .await?;
    Ok(())
}
