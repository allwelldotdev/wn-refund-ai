//! Admin request routes: the queue, the case file, the raw audit rows, messages
//! to the customer, and resolving escalations. The admin makes the final call
//! on every escalation.

use ai::{AiError, Stage};
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use db::admin::{ListFilter, RequestDetail, Resolution, Resolve, Resolved, Stats};
use db::messages::Message;
use db::settings::Settings;
use domain::money::format_cents;
use domain::notice::{NoticeInput, NoticeOutput, Outcome, continue_greeting, validate_notice};
use domain::responder::clean_reply;
use domain::types::{Flag, RequestState};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::AdminSession;
use crate::error::{ApiError, ApiJson};
use crate::{AppState, pipeline};

pub const MAX_PAGE: i64 = 200;
pub const MAX_NOTE_CHARS: usize = 2000;
/// The messages table's own limit.
pub const MAX_ADMIN_MESSAGE_CHARS: usize = 4000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/stats", get(stats))
        .route("/api/admin/requests", get(list))
        .route("/api/admin/requests/{ref}", get(detail))
        .route("/api/admin/requests/{ref}/audit", get(audit))
        .route("/api/admin/requests/{ref}/resolve", post(resolve))
        .route(
            "/api/admin/requests/{ref}/resolve/draft",
            post(draft_notice),
        )
        .route("/api/admin/requests/{ref}/messages", post(post_message))
        .route("/api/admin/requests/{ref}/read", post(mark_read))
        .route("/api/admin/settings", get(get_settings).put(put_settings))
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
    let disputed = match param(&pairs, "disputed") {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => return Err(ApiError::field("disputed", "must be true or false")),
    };
    let filter = ListFilter {
        state: filter_state,
        q: param(&pairs, "q").map(str::to_owned),
        since: time_param(&pairs, "since")?,
        flag_groups,
        disputed,
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
struct DraftBody {
    resolution: Resolution,
    note: String,
}

#[derive(Deserialize)]
struct ResolveBody {
    resolution: Resolution,
    note: String,
    /// The notice the admin previewed and confirmed (from `resolve/draft`).
    message: String,
    summary: String,
}

fn checked_note(note: &str) -> Result<&str, ApiError> {
    let note = note.trim();
    if note.is_empty() || note.chars().count() > MAX_NOTE_CHARS {
        return Err(ApiError::field(
            "note",
            format!("must be between 1 and {MAX_NOTE_CHARS} characters"),
        ));
    }
    Ok(note)
}

/// What the notice is written from, while the request can still be resolved.
async fn notice_input(
    state: &AppState,
    request_ref: &str,
    resolution: Resolution,
    note: &str,
) -> Result<NoticeInput, ApiError> {
    let facts = db::admin::notice_facts(&state.db, request_ref)
        .await?
        .ok_or(ApiError::NotFound)?;
    if facts.state != RequestState::Escalated {
        return Err(not_escalated());
    }
    let first_name = facts
        .customer_name
        .split_whitespace()
        .next()
        .unwrap_or(&facts.customer_name)
        .to_owned();
    Ok(NoticeInput {
        outcome: match resolution {
            Resolution::Approved => Outcome::Approved,
            Resolution::Denied => Outcome::Denied,
        },
        first_name,
        request_ref: request_ref.to_owned(),
        item_name: facts.item_name,
        order_ref: facts.order_ref,
        amount: facts.amount_cents.map(format_cents),
        note: note.to_owned(),
    })
}

fn not_escalated() -> ApiError {
    ApiError::conflict("not_escalated", "Only escalated requests can be resolved.")
}

fn cleaned(n: NoticeOutput, input: &NoticeInput) -> NoticeOutput {
    NoticeOutput {
        message: continue_greeting(&clean_reply(&n.message), input),
        summary: clean_reply(&n.summary),
    }
}

/// The message and summary the customer would get, for the admin to preview.
/// Without a valid notice the review can't be completed (no template).
async fn draft_notice(
    State(state): State<AppState>,
    _: AdminSession,
    Path(request_ref): Path<String>,
    ApiJson(body): ApiJson<DraftBody>,
) -> Result<Json<NoticeOutput>, ApiError> {
    let note = checked_note(&body.note)?;
    let input = notice_input(&state, &request_ref, body.resolution, note).await?;
    let input = &input;
    let assistant = &state.assistant;
    let (notice, log) = pipeline::call_with_fallback(&state.ai, Stage::Notice, |m| async move {
        let mut done = assistant.notice(input, &m).await?;
        done.output = cleaned(done.output, input);
        validate_notice(&done.output, input).map_err(|v| AiError::Rejected(v.0))?;
        Ok(done)
    })
    .await;
    notice.map(Json).ok_or_else(|| {
        tracing::warn!(request = %request_ref, failures = ?log.failures, "resolution notice failed");
        ApiError::Unavailable {
            code: "assistant_unavailable",
            message: "The assistant is unavailable, so this review can't be completed right now.",
        }
    })
}

async fn resolve(
    State(state): State<AppState>,
    admin: AdminSession,
    Path(request_ref): Path<String>,
    ApiJson(body): ApiJson<ResolveBody>,
) -> Result<Json<Resolved>, ApiError> {
    let note = checked_note(&body.note)?;
    let input = notice_input(&state, &request_ref, body.resolution, note).await?;
    let notice = cleaned(
        NoticeOutput {
            message: body.message,
            summary: body.summary,
        },
        &input,
    );
    validate_notice(&notice, &input).map_err(|v| ApiError::field("message", v.0))?;
    let resolved = db::admin::resolve_request(
        &state.db,
        &request_ref,
        admin.admin_id,
        &Resolve {
            resolution: body.resolution,
            note,
            message: &notice.message,
            summary: &notice.summary,
        },
    )
    .await
    .map_err(|e| match e {
        db::DbError::Conflict("not_escalated") => not_escalated(),
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageBody {
    body: String,
}

/// Posts the admin's message to the customer's chat exactly as written (no
/// model involved). Only while the request is escalated; Approve and Deny stay
/// available throughout.
async fn post_message(
    State(state): State<AppState>,
    admin: AdminSession,
    Path(request_ref): Path<String>,
    ApiJson(req): ApiJson<MessageBody>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
    let body = req.body.trim();
    if body.is_empty() || body.chars().count() > MAX_ADMIN_MESSAGE_CHARS {
        return Err(ApiError::field(
            "body",
            format!("must be between 1 and {MAX_ADMIN_MESSAGE_CHARS} characters"),
        ));
    }
    let message = db::admin::post_admin_message(&state.db, &request_ref, admin.admin_id, body)
        .await
        .map_err(|e| match e {
            db::DbError::Conflict("not_escalated") => ApiError::conflict(
                "not_escalated",
                "This request has been decided, so its chat is closed.",
            ),
            other => other.into(),
        })?;
    tracing::info!(request = %request_ref, admin = %admin.name, "admin message sent");
    Ok((StatusCode::CREATED, Json(message)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadBody {
    /// The last message the admin has seen.
    seq: i32,
}

/// Marks the request's customer messages up to `seq` as read by the admins.
async fn mark_read(
    State(state): State<AppState>,
    _: AdminSession,
    Path(request_ref): Path<String>,
    ApiJson(req): ApiJson<ReadBody>,
) -> Result<StatusCode, ApiError> {
    db::admin::mark_admin_read(&state.db, &request_ref, req.seq).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_settings(
    State(state): State<AppState>,
    _: AdminSession,
) -> Result<Json<Settings>, ApiError> {
    Ok(Json(db::settings::get_settings(&state.db).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsBody {
    allow_disputes: bool,
}

/// Takes effect at once; disputes already sent stay in Escalations.
async fn put_settings(
    State(state): State<AppState>,
    s: AdminSession,
    ApiJson(req): ApiJson<SettingsBody>,
) -> Result<Json<Settings>, ApiError> {
    let settings =
        db::settings::set_allow_disputes(&state.db, s.admin_id, req.allow_disputes).await?;
    tracing::info!(admin = %s.name, allow_disputes = req.allow_disputes, "settings saved");
    Ok(Json(settings))
}
