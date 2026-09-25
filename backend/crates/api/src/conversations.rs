//! Customer routes: own orders and conversations. Every lookup is scoped to the
//! session's customer; another customer's conversation is a 404, not a 403.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use db::DbError;
use db::conversations::ConversationSummary;
use db::messages::Message;
use db::orders::Order;
use domain::prescan::{MAX_MESSAGE_CHARS, prescan};
use domain::types::SignalScope;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::AppState;
use crate::auth::CustomerSession;
use crate::error::{ApiError, ApiJson};
use crate::pipeline;
use crate::sse::{self, Emitter, SseEvent};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/orders", get(list_orders))
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route("/api/conversations/{id}", get(get_conversation))
        .route("/api/conversations/{id}/messages", post(post_message))
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
