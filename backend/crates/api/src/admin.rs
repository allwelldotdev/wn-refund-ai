//! Admin request routes: the queue, the case file, the raw audit rows, and
//! resolving escalations. The admin makes the final call on every escalation.

use axum::extract::{Path, RawQuery, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use db::admin::{ListFilter, RequestDetail, Resolution, Resolved, Stats};
use domain::types::{Flag, RequestState};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::auth::AdminSession;
use crate::error::{ApiError, ApiJson};

pub const MAX_PAGE: i64 = 200;
pub const MAX_NOTE_CHARS: usize = 2000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/stats", get(stats))
        .route("/api/admin/requests", get(list))
        .route("/api/admin/requests/{ref}", get(detail))
        .route("/api/admin/requests/{ref}/audit", get(audit))
        .route("/api/admin/requests/{ref}/resolve", post(resolve))
}

/// Query parameters as (name, value) pairs; `flag` may repeat.
fn query_pairs(raw: Option<&str>) -> Vec<(String, String)> {
    form_urlencoded::parse(raw.unwrap_or_default().as_bytes())
        .into_owned()
        .collect()
}

/// The last non-empty value of a parameter.
fn param<'a>(pairs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    pairs
        .iter()
        .rev()
        .find(|(k, v)| k == name && !v.trim().is_empty())
        .map(|(_, v)| v.trim())
}

fn int_param(pairs: &[(String, String)], name: &'static str) -> Result<Option<i64>, ApiError> {
    param(pairs, name)
        .map(|v| {
            v.parse::<i64>()
                .map_err(|_| ApiError::field(name, "must be a whole number"))
        })
        .transpose()
}

fn time_param(
    pairs: &[(String, String)],
    name: &'static str,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    param(pairs, name)
        .map(|v| {
            DateTime::parse_from_rfc3339(v)
                .map(|t| t.with_timezone(&Utc))
                .map_err(|_| ApiError::field(name, "must be an RFC 3339 timestamp"))
        })
        .transpose()
}

fn one_of<T: Copy>(all: &[T], as_str: fn(&T) -> &'static str) -> String {
    all.iter().map(as_str).collect::<Vec<_>>().join(", ")
}

/// `GET /api/admin/requests?state=&q=&since=&flag=a,b&flag=c&limit=&offset=`.
/// Each `flag` is an any-of group; a request must match every group.
async fn list(
    State(state): State<AppState>,
    _: AdminSession,
    RawQuery(raw): RawQuery,
) -> Result<Json<Value>, ApiError> {
    let pairs = query_pairs(raw.as_deref());
    let filter_state = param(&pairs, "state")
        .map(|s| {
            s.parse::<RequestState>().map_err(|_| {
                ApiError::field(
                    "state",
                    format!(
                        "must be one of {}",
                        one_of(RequestState::ALL, |s| s.as_str())
                    ),
                )
            })
        })
        .transpose()?;
    let mut flag_groups = Vec::new();
    for (_, value) in pairs
        .iter()
        .filter(|(k, v)| k == "flag" && !v.trim().is_empty())
    {
        let group = value
            .split(',')
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(|f| {
                f.parse::<Flag>().map_err(|_| {
                    ApiError::field(
                        "flag",
                        format!("must be one of {}", one_of(Flag::ALL, |f| f.as_str())),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !group.is_empty() {
            flag_groups.push(group);
        }
    }
    let limit = int_param(&pairs, "limit")?.unwrap_or(50);
    if !(1..=MAX_PAGE).contains(&limit) {
        return Err(ApiError::field(
            "limit",
            format!("must be between 1 and {MAX_PAGE}"),
        ));
    }
    let offset = int_param(&pairs, "offset")?.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::field("offset", "must not be negative"));
    }
    let filter = ListFilter {
        state: filter_state,
        q: param(&pairs, "q").map(str::to_owned),
        since: time_param(&pairs, "since")?,
        flag_groups,
        limit,
        offset,
    };
    let (items, total) = db::admin::list_requests(&state.db, &filter).await?;
    Ok(Json(json!({ "items": items, "total": total })))
}

/// `GET /api/admin/stats?since=` (default: the last 24 hours).
async fn stats(
    State(state): State<AppState>,
    _: AdminSession,
    RawQuery(raw): RawQuery,
) -> Result<Json<Stats>, ApiError> {
    let pairs = query_pairs(raw.as_deref());
    let since =
        time_param(&pairs, "since")?.unwrap_or_else(|| Utc::now() - chrono::Duration::hours(24));
    Ok(Json(db::admin::stats(&state.db, since).await?))
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
