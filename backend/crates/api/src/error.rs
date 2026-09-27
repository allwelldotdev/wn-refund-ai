//! One error type for every handler. The body is always
//! `{"error":{"code","message",...}}`; internal details are logged, never sent.

use std::time::Duration;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use db::DbError;
use domain::policy::FieldError;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    /// `attempts_left` before sign-in for this email pauses.
    InvalidCredentials {
        attempts_left: u32,
    },
    Forbidden(&'static str),
    NotFound,
    /// `extra` is merged into the error object (e.g. `latest` for `stale_base`).
    Conflict {
        code: &'static str,
        message: String,
        extra: Map<String, Value>,
    },
    Unprocessable(Vec<FieldError>),
    /// A body the JSON extractor refused (malformed, wrong type, too large).
    BadBody {
        status: StatusCode,
        message: String,
    },
    RateLimited(Duration),
    /// Too many failed sign-ins for one email.
    SignInPaused(Duration),
    /// A dependency (e.g. the model) failed; nothing was changed.
    Unavailable {
        code: &'static str,
        message: &'static str,
    },
    Internal(anyhow::Error),
}

impl ApiError {
    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        ApiError::Conflict {
            code,
            message: message.into(),
            extra: Map::new(),
        }
    }

    pub fn field(path: impl Into<String>, message: impl Into<String>) -> Self {
        ApiError::Unprocessable(vec![FieldError {
            path: path.into(),
            message: message.into(),
        }])
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, extra) = match self {
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Sign in to continue.".to_owned(),
                Map::new(),
            ),
            ApiError::InvalidCredentials { attempts_left } => {
                let mut extra = Map::new();
                extra.insert("attempts_left".into(), attempts_left.into());
                (
                    StatusCode::UNAUTHORIZED,
                    "invalid_credentials",
                    "Email or password is incorrect.".to_owned(),
                    extra,
                )
            }
            ApiError::Forbidden(code) => (
                StatusCode::FORBIDDEN,
                code,
                "You do not have access to this resource.".to_owned(),
                Map::new(),
            ),
            ApiError::NotFound => (
                StatusCode::NOT_FOUND,
                "not_found",
                "Not found.".to_owned(),
                Map::new(),
            ),
            ApiError::Conflict {
                code,
                message,
                extra,
            } => (StatusCode::CONFLICT, code, message, extra),
            ApiError::Unprocessable(fields) => {
                let mut extra = Map::new();
                extra.insert("fields".into(), json!(fields));
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "validation_failed",
                    "Some fields are invalid.".to_owned(),
                    extra,
                )
            }
            ApiError::BadBody { status, message } => (status, "invalid_body", message, Map::new()),
            ApiError::RateLimited(retry_after) => {
                return too_many(retry_after, |secs| {
                    format!("Too many messages. Try again in {secs} seconds.")
                });
            }
            ApiError::SignInPaused(retry_after) => {
                return too_many(retry_after, |secs| {
                    format!("Too many sign-in attempts. Try again in {secs} seconds.")
                });
            }
            ApiError::Unavailable { code, message } => (
                StatusCode::SERVICE_UNAVAILABLE,
                code,
                message.to_owned(),
                Map::new(),
            ),
            ApiError::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "Something went wrong. Please try again.".to_owned(),
                    Map::new(),
                )
            }
        };
        error_response(status, code, message, extra)
    }
}

/// 429 `rate_limited` with a `Retry-After` header in whole seconds (at least 1).
fn too_many(retry_after: Duration, message: impl FnOnce(u64) -> String) -> Response {
    let secs = retry_after.as_secs_f64().ceil().max(1.0) as u64;
    let mut response = error_response(
        StatusCode::TOO_MANY_REQUESTS,
        "rate_limited",
        message(secs),
        Map::new(),
    );
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from(secs));
    response
}

fn error_response(
    status: StatusCode,
    code: &str,
    message: String,
    extra: Map<String, Value>,
) -> Response {
    let mut error = Map::new();
    error.insert("code".into(), code.into());
    error.insert("message".into(), message.into());
    error.extend(extra);
    (status, Json(json!({ "error": error }))).into_response()
}

impl From<DbError> for ApiError {
    fn from(err: DbError) -> Self {
        match err {
            DbError::NotFound => ApiError::NotFound,
            DbError::Conflict(code) => ApiError::conflict(code, code.replace('_', " ")),
            other => ApiError::Internal(other.into()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err)
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        let status = match rejection.status() {
            StatusCode::BAD_REQUEST => StatusCode::UNPROCESSABLE_ENTITY,
            other => other,
        };
        ApiError::BadBody {
            status,
            message: rejection.body_text(),
        }
    }
}

/// `axum::Json` with rejections in the API's error format.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state).await?;
        Ok(ApiJson(value))
    }
}
