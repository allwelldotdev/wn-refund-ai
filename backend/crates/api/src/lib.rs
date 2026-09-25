//! HTTP API for the refund system. Handlers authenticate with the extractors in
//! `auth`, return `ApiError` on failure, and never decide a refund themselves:
//! the message pipeline hands that to `domain::engine::decide`.

pub mod auth;
pub mod config;
pub mod conversations;
pub mod error;
pub mod policy;

use std::sync::Arc;

use ai::{AiConfig, SharedAssistant};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use db::Db;
use serde_json::{Value, json};
use tower_http::trace::TraceLayer;

use crate::error::ApiError;

/// Seed source for policy version 1 (ADR-016). Compiled in so the container
/// needs no extra files at runtime.
pub const DEFAULT_POLICY_JSON: &str = include_str!("../../../../policy/default-policy.json");

/// Request bodies above this are rejected with 413.
pub const BODY_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub assistant: SharedAssistant,
    pub ai: Arc<AiConfig>,
}

/// Applies migrations, then the idempotent seed. Runs on every startup.
pub async fn prepare_database(db: &Db) -> anyhow::Result<db::seed::SeedReport> {
    db::migrate(db).await?;
    Ok(db::seed::run(db, DEFAULT_POLICY_JSON).await?)
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .merge(auth::routes())
        .merge(policy::routes())
        .merge(conversations::routes())
        .fallback(|| async { ApiError::NotFound })
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    match db::ping(&state.db).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(e) => {
            tracing::warn!(error = %e, "health check: database unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
        }
    }
}
