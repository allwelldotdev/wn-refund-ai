//! Customer routes: own orders and conversations. Every lookup is scoped to the
//! session's customer; another customer's conversation is a 404, not a 403.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use db::DbError;
use db::conversations::{ConversationSummary, RequestSummary};
use db::messages::Message;
use db::orders::Order;
use domain::prescan::{MAX_MESSAGE_CHARS, prescan};
use domain::types::{MessageRole, RequestState, SignalScope};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::AppState;
use crate::auth::CustomerSession;
use crate::error::{ApiError, ApiJson};
use crate::sse::{self, Emitter, SseEvent};
use crate::{pipeline, review_job};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/orders", get(list_orders))
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route("/api/conversations/{id}", get(get_conversation))
        .route("/api/conversations/{id}/messages", post(post_message))
        .route("/api/conversations/{id}/dispute", post(dispute))
        .route("/api/conversations/{id}/read", post(mark_read))
}

async fn list_orders(
    State(state): State<AppState>,
    s: CustomerSession,
) -> Result<Json<Vec<Order>>, ApiError> {
    Ok(Json(
        db::orders::list_orders_for_customer(&state.db, s.customer_id).await?,
    ))
}

async fn list_conversations(
    State(state): State<AppState>,
    s: CustomerSession,
) -> Result<Json<Vec<ConversationSummary>>, ApiError> {
    Ok(Json(
        db::conversations::list_conversations(&state.db, s.customer_id).await?,
    ))
}

async fn create_conversation(
    State(state): State<AppState>,
    s: CustomerSession,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let c = db::conversations::create_conversation(&state.db, s.customer_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": c.id, "created_at": c.created_at })),
    ))
}

async fn get_conversation(
    State(state): State<AppState>,
    s: CustomerSession,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let conversation = db::conversations::get_conversation_owned(&state.db, id, s.customer_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let messages = db::messages::list_messages(&state.db, id).await?;
    let request = db::conversations::find_request_summary(&state.db, id).await?;
    Ok(Json(json!({
        "conversation": conversation,
        "messages": messages,
        "request": request,
    })))
}

#[derive(Deserialize)]
struct PostMessage {
    client_msg_id: Uuid,
    body: String,
    order_id: Option<Uuid>,
}

/// Stores the message with its per-message pre-scan, then streams the
/// pipeline's reply as SSE. Retrying with the same `client_msg_id` never
/// stores or decides twice.
async fn post_message(
    State(state): State<AppState>,
    s: CustomerSession,
    Path(conversation_id): Path<Uuid>,
    ApiJson(req): ApiJson<PostMessage>,
) -> Result<Response, ApiError> {
    state
        .rate
        .check(s.customer_id)
        .map_err(ApiError::RateLimited)?;
    let body = req.body.trim();
    if body.is_empty() {
        return Err(ApiError::field("body", "must not be empty"));
    }
    if body.chars().count() > MAX_MESSAGE_CHARS {
        return Err(ApiError::field(
            "body",
            format!("must be at most {MAX_MESSAGE_CHARS} characters"),
        ));
    }
    if let Some(order_id) = req.order_id
        && !db::orders::is_order_owned(&state.db, order_id, s.customer_id).await?
    {
        return Err(ApiError::Forbidden("order_not_owned"));
    }
    db::conversations::get_conversation_owned(&state.db, conversation_id, s.customer_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if let Some(existing) =
        db::messages::find_message_by_client_id(&state.db, req.client_msg_id).await?
    {
        return duplicate(existing, conversation_id);
    }
    // Approved and denied requests are closed; an escalated one stays open so
    // the customer can add details for the specialist.
    if let Some(request) =
        db::refunds::find_request_for_conversation(&state.db, conversation_id).await?
        && request.state != RequestState::Escalated
    {
        return Err(ApiError::conflict(
            "request_closed",
            "This request is closed. Start a new request to ask about something else.",
        ));
    }

    let mut tx = state.db.0.begin().await.map_err(DbError::from)?;
    let message = match db::messages::insert_customer_message(
        &mut tx,
        conversation_id,
        req.client_msg_id,
        body,
        req.order_id,
    )
    .await
    {
        Ok(m) => m,
        // A retry raced this request and won; answer as for any duplicate.
        Err(DbError::Conflict("duplicate_client_msg_id")) => {
            drop(tx);
            let existing = db::messages::find_message_by_client_id(&state.db, req.client_msg_id)
                .await?
                .ok_or(ApiError::NotFound)?;
            return duplicate(existing, conversation_id);
        }
        Err(e) => return Err(e.into()),
    };
    db::messages::insert_signals(
        &mut tx,
        message.id,
        SignalScope::Message,
        &prescan(&message.body),
    )
    .await?;
    tx.commit().await.map_err(DbError::from)?;

    let (tx, rx) = mpsc::channel(64);
    let out = Emitter(tx);
    out.send(SseEvent::MessageSaved {
        message_id: message.id,
        seq: message.seq,
        duplicate: false,
    })
    .await;
    tokio::spawn(pipeline::run_message(
        state,
        conversation_id,
        s.customer_id,
        message,
        out,
    ));
    Ok(sse::stream(rx).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DisputeBody {
    reason: Option<String>,
}

/// The customer asks a person to review an automatic denial: once, and only
/// while the admin setting allows it. The optional reason is stored and
/// pre-scanned like any customer message, a system note records the dispute,
/// and the request goes back to Escalations with a fresh review draft.
async fn dispute(
    State(state): State<AppState>,
    s: CustomerSession,
    Path(conversation_id): Path<Uuid>,
    ApiJson(req): ApiJson<DisputeBody>,
) -> Result<Json<RequestSummary>, ApiError> {
    let reason = req
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty());
    if reason.is_some_and(|r| r.chars().count() > MAX_MESSAGE_CHARS) {
        return Err(ApiError::field(
            "reason",
            format!("must be at most {MAX_MESSAGE_CHARS} characters"),
        ));
    }
    db::conversations::get_conversation_owned(&state.db, conversation_id, s.customer_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let mut tx = state.db.0.begin().await.map_err(DbError::from)?;
    let target = db::refunds::lock_for_dispute(&mut tx, conversation_id)
        .await?
        .filter(|t| t.state == RequestState::Denied || t.disputed_at.is_some())
        .ok_or_else(|| {
            ApiError::conflict(
                "not_disputable",
                "Only a request the assistant denied can be disputed.",
            )
        })?;
    if target.disputed_at.is_some() {
        return Err(ApiError::conflict(
            "already_disputed",
            "This decision has already been disputed.",
        ));
    }
    if !target.allow_disputes {
        return Err(ApiError::conflict(
            "disputes_off",
            "Automatic decisions are final at the moment.",
        ));
    }
    if let Some(reason) = reason {
        let message = db::messages::insert_customer_message(
            &mut tx,
            conversation_id,
            Uuid::new_v4(),
            reason,
            None,
        )
        .await?;
        db::messages::insert_signals(&mut tx, message.id, SignalScope::Message, &prescan(reason))
            .await?;
    }
    let today = Utc::now().format("%b %-d, %Y");
    db::messages::insert_note(
        &mut tx,
        conversation_id,
        MessageRole::System,
        &format!(
            "You disputed this decision on {today}. A support specialist will review it and reply here."
        ),
        None,
    )
    .await?;
    db::refunds::mark_disputed(&mut tx, target.id).await?;
    tx.commit().await.map_err(DbError::from)?;

    tracing::info!(%conversation_id, "automatic denial disputed");
    tokio::spawn(review_job::run_review(state.clone(), target.id));
    let summary = db::conversations::find_request_summary(&state.db, conversation_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(summary))
}

/// Same conversation: acknowledge and end; the client re-reads the
/// conversation. Another conversation: the id was reused, which is a client bug.
fn duplicate(existing: Message, conversation_id: Uuid) -> Result<Response, ApiError> {
    if existing.conversation_id != conversation_id {
        return Err(ApiError::conflict(
            "client_msg_id_reused",
            "This message id was already used in another conversation.",
        ));
    }
    let (tx, rx) = mpsc::channel(2);
    let saved = SseEvent::MessageSaved {
        message_id: existing.id,
        seq: existing.seq,
        duplicate: true,
    };
    for event in [saved, SseEvent::Done] {
        tx.try_send(event)
            .expect("channel has room for both events");
    }
    Ok(sse::stream(rx).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadBody {
    /// The last message the customer has seen.
    seq: i32,
}

/// Marks the conversation's messages up to `seq` as read by the customer.
async fn mark_read(
    State(state): State<AppState>,
    s: CustomerSession,
    Path(id): Path<Uuid>,
    ApiJson(req): ApiJson<ReadBody>,
) -> Result<StatusCode, ApiError> {
    db::conversations::mark_customer_read(&state.db, id, s.customer_id, req.seq).await?;
    Ok(StatusCode::NO_CONTENT)
}
