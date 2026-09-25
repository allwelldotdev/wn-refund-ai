//! Customer routes: own orders and conversations. Every lookup is scoped to the
//! session's customer; another customer's conversation is a 404, not a 403.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use db::conversations::ConversationSummary;
use db::orders::Order;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::CustomerSession;
use crate::error::ApiError;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/orders", get(list_orders))
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route("/api/conversations/{id}", get(get_conversation))
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
