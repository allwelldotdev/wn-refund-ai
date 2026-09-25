//! Policy endpoints. Customers read the current prose; admins edit typed rules.
//! Prose always comes from `domain::prose::render_policy`, so what customers
//! read is exactly what the engine enforces.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use db::DbError;
use db::policy::{NewPolicyVersion, PolicyVersionRow};
use domain::policy::Policy;
use domain::prose::render_policy;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AdminSession, AnySession};
use crate::error::{ApiError, ApiJson};

pub const MAX_CHANGE_NOTE_CHARS: usize = 500;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/policy", get(public_policy))
        .route("/api/admin/policy/current", get(current))
        .route("/api/admin/policy/versions", get(list).post(create))
        .route("/api/admin/policy/versions/{id}", get(get_version))
        .route("/api/admin/policy/versions/{id}/revert", post(revert))
        .route("/api/admin/policy/preview", post(preview))
}

fn version_json(v: &PolicyVersionRow) -> Value {
    json!({ "version": v.meta, "rules": v.rules, "prose": render_policy(&v.rules) })
}

/// Rules arrive as raw JSON so that every problem, including unknown kinds and
/// wrong types, comes back as a field error with a path.
fn parse_rules(rules: &Value) -> Result<Policy, ApiError> {
    Policy::parse(&rules.to_string()).map_err(ApiError::Unprocessable)
}

fn change_note(note: Option<String>) -> Result<Option<String>, ApiError> {
    let note = note.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
    if note
        .as_ref()
        .is_some_and(|n| n.chars().count() > MAX_CHANGE_NOTE_CHARS)
    {
        return Err(ApiError::field(
            "change_note",
            format!("must be at most {MAX_CHANGE_NOTE_CHARS} characters"),
        ));
    }
    Ok(note)
}

async fn public_policy(
    State(state): State<AppState>,
    _: AnySession,
) -> Result<Json<Value>, ApiError> {
    let v = db::policy::latest_policy(&state.db).await?;
    Ok(Json(
        json!({ "version": v.meta.version, "prose": render_policy(&v.rules) }),
    ))
}

async fn current(State(state): State<AppState>, _: AdminSession) -> Result<Json<Value>, ApiError> {
    Ok(Json(version_json(
        &db::policy::latest_policy(&state.db).await?,
    )))
}

async fn list(State(state): State<AppState>, _: AdminSession) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(
        db::policy::list_policy_versions(&state.db).await?
    )))
}

async fn get_version(
    State(state): State<AppState>,
    _: AdminSession,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let v = db::policy::get_policy_version(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(version_json(&v)))
}

#[derive(Deserialize)]
struct CreateBody {
    base_version_id: Uuid,
    rules: Value,
    change_note: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    admin: AdminSession,
    ApiJson(body): ApiJson<CreateBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let rules = parse_rules(&body.rules)?;
    let note = change_note(body.change_note)?;
    let new = NewPolicyVersion {
        rules: &rules,
        base_version_id: Some(body.base_version_id),
        author_admin_id: admin.admin_id,
        change_note: note.as_deref(),
        reverted_from: None,
    };
    insert(&state, new).await
}

#[derive(Deserialize)]
struct RevertBody {
    change_note: Option<String>,
}

/// Inserts a copy of an old version on top of the latest one.
async fn revert(
    State(state): State<AppState>,
    admin: AdminSession,
    Path(id): Path<Uuid>,
    ApiJson(body): ApiJson<RevertBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let target = db::policy::get_policy_version(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let note = change_note(body.change_note)?
        .unwrap_or_else(|| format!("Reverted to version {}", target.meta.version));
    let new = NewPolicyVersion {
        rules: &target.rules,
        base_version_id: None,
        author_admin_id: admin.admin_id,
        change_note: Some(&note),
        reverted_from: Some(target.meta.id),
    };
    insert(&state, new).await
}

async fn insert(
    state: &AppState,
    new: NewPolicyVersion<'_>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    match db::policy::insert_policy_version(&state.db, new).await {
        Ok(v) => Ok((StatusCode::CREATED, Json(version_json(&v)))),
        Err(DbError::StaleBase { latest_version, .. }) => {
            let latest = db::policy::latest_policy(&state.db).await?;
            let mut extra = Map::new();
            extra.insert("latest".into(), version_json(&latest));
            Err(ApiError::Conflict {
                code: "stale_base",
                message: format!(
                    "The policy changed while you were editing: version {latest_version} is now the latest."
                ),
                extra,
            })
        }
        Err(DbError::NoOp) => Err(ApiError::conflict(
            "no_op",
            "These rules are the same as the current policy.",
        )),
        Err(e) => Err(e.into()),
    }
}

#[derive(Deserialize)]
struct PreviewBody {
    rules: Value,
}

async fn preview(
    _: AdminSession,
    ApiJson(body): ApiJson<PreviewBody>,
) -> Result<Json<Value>, ApiError> {
    let rules = parse_rules(&body.rules)?;
    Ok(Json(json!({ "prose": render_policy(&rules) })))
}
