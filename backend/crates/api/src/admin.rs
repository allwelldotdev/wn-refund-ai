//! Admin request routes: the queue, the case file, the raw audit rows, and
//! resolving escalations. The admin makes the final call on every escalation.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use db::admin::{ListFilter, RequestDetail, Resolution, Resolved};
use domain::types::RequestState;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::auth::AdminSession;
use crate::error::{ApiError, ApiJson};

pub const MAX_PAGE: i64 = 200;
pub const MAX_NOTE_CHARS: usize = 2000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/requests", get(list))
        .route("/api/admin/requests/{ref}", get(detail))
        .route("/api/admin/requests/{ref}/audit", get(audit))
        .route("/api/admin/requests/{ref}/resolve", post(resolve))
}

#[derive(Deserialize)]
struct ListQuery {
    state: Option<String>,
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    _: AdminSession,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let filter_state = match query.state.as_deref().filter(|s| !s.is_empty()) {
        None => None,
        Some(s) => Some(s.parse::<RequestState>().map_err(|_| {
            ApiError::field(
                "state",
                format!(
                    "must be one of {}",
                    RequestState::ALL
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?),
    };
    let limit = query.limit.unwrap_or(50);
    if !(1..=MAX_PAGE).contains(&limit) {
        return Err(ApiError::field(
            "limit",
            format!("must be between 1 and {MAX_PAGE}"),
        ));
    }
    let offset = query.offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::field("offset", "must not be negative"));
    }
    let filter = ListFilter {
        state: filter_state,
        q: query.q,
        limit,
        offset,
    };
    let (items, total) = db::admin::list_requests(&state.db, &filter).await?;
    Ok(Json(json!({ "items": items, "total": total })))
}

async fn detail(
    State(state): State<AppState>,
    _: AdminSession,
    Path(request_ref): Path<String>,
) -> Result<Json<RequestDetail>, ApiError> {
    db::admin::get_request_detail(&state.db, &request_ref)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn audit(
    State(state): State<AppState>,
    _: AdminSession,
    Path(request_ref): Path<String>,
) -> Result<Json<Value>, ApiError> {
    db::admin::get_request_audit_raw(&state.db, &request_ref)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize)]
struct ResolveBody {
    resolution: Resolution,
    note: String,
}

async fn resolve(
    State(state): State<AppState>,
    admin: AdminSession,
    Path(request_ref): Path<String>,
    ApiJson(body): ApiJson<ResolveBody>,
) -> Result<Json<Resolved>, ApiError> {
    let note = body.note.trim();
    if note.is_empty() || note.chars().count() > MAX_NOTE_CHARS {
        return Err(ApiError::field(
            "note",
            format!("must be between 1 and {MAX_NOTE_CHARS} characters"),
        ));
    }
    let resolved = db::admin::resolve_request(
        &state.db,
        &request_ref,
        admin.admin_id,
        body.resolution,
        note,
    )
    .await
    .map_err(|e| match e {
        db::DbError::Conflict("not_escalated") => {
            ApiError::conflict("not_escalated", "Only escalated requests can be resolved.")
        }
        db::DbError::Conflict("duplicate_active_refund") => ApiError::conflict(
            "duplicate_active_refund",
            "This item already has an approved refund.",
        ),
        other => other.into(),
    })?;
    tracing::info!(
        request = %resolved.request_ref,
        state = %resolved.state,
        admin = %admin.name,
        "escalation resolved"
    );
    Ok(Json(resolved))
}
