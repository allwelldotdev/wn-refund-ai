//! Bearer sessions. The token is the `sessions.id` uuid; the Next.js BFF keeps
//! it in an httpOnly per-tab cookie and forwards it. Every protected handler
//! takes one of the extractors below, so role checks live in one place.

use std::sync::LazyLock;
use std::time::Duration;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use axum::extract::{FromRequestParts, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::routing::{get, post};
use axum::{Json, Router};
use db::auth::{DemoAccount, Principal, Role};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::error::{ApiError, ApiJson};

pub const SESSION_TTL: Duration = Duration::from_secs(24 * 3600);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/auth/demo-accounts", get(demo_accounts))
}

/// Any signed-in principal.
pub struct AnySession {
    pub principal: Principal,
    pub session_id: Uuid,
}

pub struct CustomerSession {
    pub customer_id: Uuid,
    pub session_id: Uuid,
}

pub struct AdminSession {
    pub admin_id: Uuid,
    pub name: String,
    pub session_id: Uuid,
}

impl FromRequestParts<AppState> for AnySession {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let session_id = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .and_then(|t| Uuid::parse_str(t.trim()).ok())
            .ok_or(ApiError::Unauthorized)?;
        let principal = db::auth::find_valid_session(&state.db, session_id)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        Ok(AnySession {
            principal,
            session_id,
        })
    }
}

impl FromRequestParts<AppState> for CustomerSession {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let s = AnySession::from_request_parts(parts, state).await?;
        match s.principal.kind {
            Role::Customer => Ok(CustomerSession {
                customer_id: s.principal.id,
                session_id: s.session_id,
            }),
            Role::Admin => Err(ApiError::Forbidden("customer_only")),
        }
    }
}

impl FromRequestParts<AppState> for AdminSession {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let s = AnySession::from_request_parts(parts, state).await?;
        match s.principal.kind {
            Role::Admin => Ok(AdminSession {
                admin_id: s.principal.id,
                name: s.principal.name,
                session_id: s.session_id,
            }),
            Role::Customer => Err(ApiError::Forbidden("admin_only")),
        }
    }
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

/// Verified against when the email is unknown, so both failure paths cost one
/// argon2 verification and take about the same time.
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    Argon2::default()
        .hash_password(b"no account has this password")
        .expect("argon2 hashes a constant")
        .to_string()
});

async fn login(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<LoginBody>,
) -> Result<Json<Value>, ApiError> {
    let email = body.email.trim().to_lowercase();
    let account = db::auth::find_account(&state.db, &email).await?;
    let hash = account
        .as_ref()
        .map_or_else(|| DUMMY_HASH.clone(), |a| a.password_hash.clone());
    let password = body.password;
    let verified = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .map_err(anyhow::Error::from)?;

    let Some(account) = account.filter(|_| verified) else {
        return Err(ApiError::InvalidCredentials);
    };
    let expires_at = chrono::Utc::now() + SESSION_TTL;
    let token = db::auth::create_session(&state.db, &account.principal, expires_at).await?;
    Ok(Json(
        json!({ "token": token, "principal": account.principal }),
    ))
}

async fn logout(State(state): State<AppState>, s: AnySession) -> Result<Json<Value>, ApiError> {
    db::auth::delete_session(&state.db, s.session_id).await?;
    Ok(Json(json!({})))
}

async fn me(s: AnySession) -> Json<Value> {
    Json(json!({ "principal": s.principal }))
}

async fn demo_accounts(State(state): State<AppState>) -> Result<Json<Vec<DemoAccount>>, ApiError> {
    Ok(Json(db::auth::list_demo_accounts(&state.db).await?))
}
