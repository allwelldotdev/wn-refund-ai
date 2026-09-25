//! HTTP API for the refund system. Milestone 1 provides startup (migrate + seed)
//! and the health endpoint; the remaining routes arrive in milestone 3.

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use db::Db;
use serde_json::{Value, json};
use tower_http::trace::TraceLayer;

/// Seed source for policy version 1 (ADR-016). Compiled in so the container
/// needs no extra files at runtime.
pub const DEFAULT_POLICY_JSON: &str = include_str!("../../../../policy/default-policy.json");

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}

/// Applies migrations, then the idempotent seed. Runs on every startup.
pub async fn prepare_database(db: &Db) -> anyhow::Result<db::seed::SeedReport> {
    db::migrate(db).await?;
    Ok(db::seed::run(db, DEFAULT_POLICY_JSON).await?)
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
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
