//! Postgres access for the refund system: pool, migrations, queries and the
//! idempotent demo seed. Queries use SQLx compile-time checking; the offline
//! metadata in `backend/.sqlx` lets Docker builds compile without a database.

pub mod seed;

use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

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
}

pub async fn connect(database_url: &str) -> Result<Db, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
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
