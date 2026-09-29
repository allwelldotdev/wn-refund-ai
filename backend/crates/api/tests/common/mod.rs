//! Shared harness: the real router on a fresh seeded `#[sqlx::test]` database,
//! with `FakeAssistant` in place of the LLM. Requests go through
//! `tower::ServiceExt::oneshot`, so no port is bound.

#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ai::{
    AiConfig, AiError, Completed, FakeAssistant, RefundAssistant, SharedAssistant, StageModel,
};
use api::rate_limit::RateLimiter;
use api::{AppState, build_router};
use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use db::Db;
use domain::intake::{IntakeInput, IntakeOutput, IntakeStatus, Intent, MissingField};
use domain::notice::{NoticeInput, NoticeOutput};
use domain::responder::ResponderInput;
use domain::review::{ReviewInput, ReviewOutput};
use domain::types::ReasonCategory;
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tokio::sync::Notify;
use tower::ServiceExt;
use uuid::Uuid;

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    pub fake: Arc<FakeAssistant>,
    pub pool: PgPool,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl TestResponse {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "body is not JSON ({e}): {}",
                String::from_utf8_lossy(&self.body)
            )
        })
    }

    pub fn error_code(&self) -> String {
        self.json()["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// Parses a `text/event-stream` body into `(event, data)` pairs, skipping
    /// keep-alive comments.
    pub fn events(&self) -> Vec<(String, Value)> {
        let text = String::from_utf8_lossy(&self.body);
        text.split("\n\n")
            .filter_map(|frame| {
                let mut event = None;
                let mut data = String::new();
                for line in frame.lines() {
                    if let Some(v) = line.strip_prefix("event:") {
                        event = Some(v.trim().to_owned());
                    } else if let Some(v) = line.strip_prefix("data:") {
                        data.push_str(v.trim_start());
                    }
                }
                let data = if data.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_str(&data).expect("SSE data is JSON")
                };
                event.map(|e| (e, data))
            })
            .collect()
    }

    pub fn event_names(&self) -> Vec<String> {
        self.events().into_iter().map(|(e, _)| e).collect()
    }

    pub fn event(&self, name: &str) -> Value {
        self.events()
            .into_iter()
            .find(|(e, _)| e == name)
            .map(|(_, d)| d)
            .unwrap_or_else(|| panic!("no `{name}` event in {:?}", self.event_names()))
    }
}

impl TestApp {
    pub async fn new(pool: PgPool) -> TestApp {
        let fake = Arc::new(FakeAssistant::new());
        TestApp::with_assistant(pool, fake.clone(), fake).await
    }

    /// `assistant` answers the pipeline; `fake` is the queue it draws on.
    pub async fn with_assistant(
        pool: PgPool,
        assistant: SharedAssistant,
        fake: Arc<FakeAssistant>,
    ) -> TestApp {
        let db = Db(pool.clone());
        api::prepare_database(&db).await.expect("seed");
        let state = AppState {
            db,
            assistant,
            ai: Arc::new(AiConfig::load().expect("default AI config")),
            rate: RateLimiter::default(),
            login_throttle: Default::default(),
        };
        TestApp {
            router: build_router(state.clone()),
            state,
            fake,
            pool,
        }
    }

    pub async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&Value>,
    ) -> TestResponse {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let body = match body {
            Some(v) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(serde_json::to_vec(v).unwrap())
            }
            None => Body::empty(),
        };
        let res = self
            .router
            .clone()
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        TestResponse {
            status,
            headers,
            body,
        }
    }

    pub async fn get(&self, path: &str, token: &str) -> TestResponse {
        self.call(Method::GET, path, Some(token), None).await
    }

    pub async fn post(&self, path: &str, token: &str, body: Value) -> TestResponse {
        self.call(Method::POST, path, Some(token), Some(&body))
            .await
    }

    /// Drafts the customer notice, then resolves with it, as the admin UI
    /// does. Returns the draft's response if drafting fails.
    pub async fn resolve(
        &self,
        token: &str,
        request_ref: &str,
        resolution: &str,
        note: &str,
    ) -> TestResponse {
        let path = format!("/api/admin/requests/{request_ref}/resolve");
        let draft = self
            .post(
                &format!("{path}/draft"),
                token,
                serde_json::json!({ "resolution": resolution, "note": note }),
            )
            .await;
        if draft.status != StatusCode::OK {
            return draft;
        }
        let d = draft.json();
        self.post(
            &path,
            token,
            serde_json::json!({
                "resolution": resolution, "note": note,
                "message": d["message"], "summary": d["summary"],
            }),
        )
        .await
    }

    /// Signs in with the demo password and returns the bearer token.
    pub async fn login(&self, email: &str) -> String {
        let body = serde_json::json!({ "email": email, "password": db::seed::DEMO_PASSWORD });
        let res = self
            .call(Method::POST, "/api/auth/login", None, Some(&body))
            .await;
        assert_eq!(res.status, StatusCode::OK, "login {email}");
        res.json()["token"].as_str().unwrap().to_owned()
    }
}

impl TestApp {
    pub async fn new_conversation(&self, token: &str) -> String {
        let res = self
            .post("/api/conversations", token, serde_json::json!({}))
            .await;
        assert_eq!(res.status, StatusCode::CREATED);
        res.json()["id"].as_str().unwrap().to_owned()
    }

    /// Posts a customer message with a fresh `client_msg_id` and returns the
    /// whole SSE response (the stream ends after `done`).
    pub async fn say(&self, token: &str, conversation: &str, body: &str) -> TestResponse {
        self.say_with(token, conversation, body, None, Uuid::new_v4())
            .await
    }

    pub async fn say_with(
        &self,
        token: &str,
        conversation: &str,
        body: &str,
        order_id: Option<Uuid>,
        client_msg_id: Uuid,
    ) -> TestResponse {
        let mut req = serde_json::json!({ "client_msg_id": client_msg_id, "body": body });
        if let Some(id) = order_id {
            req["order_id"] = id.to_string().into();
        }
        self.post(
            &format!("/api/conversations/{conversation}/messages"),
            token,
            req,
        )
        .await
    }

    /// Sends a complete request (intake scripted as `intake`) and answers the
    /// assistant's final question, returning the stream with the decision.
    pub async fn decide(
        &self,
        token: &str,
        conversation: &str,
        body: &str,
        order_id: Option<Uuid>,
        intake: IntakeOutput,
    ) -> TestResponse {
        self.fake.push_intake(Ok(intake.clone()));
        let res = self
            .say_with(token, conversation, body, order_id, Uuid::new_v4())
            .await;
        assert_eq!(res.event("reply_start")["kind"], "final_check");
        self.confirm(token, conversation, intake).await
    }

    /// Answers the final question with "No, that's all."; intake repeats
    /// `intake` with the customer done.
    pub async fn confirm(
        &self,
        token: &str,
        conversation: &str,
        mut intake: IntakeOutput,
    ) -> TestResponse {
        intake.intent = Intent::Finished;
        self.fake.push_intake(Ok(intake));
        self.say(token, conversation, "No, that's all.").await
    }

    /// The conversation's `decision_audit` row as JSON.
    pub async fn audit(&self, conversation: &str) -> Value {
        sqlx::query_scalar(
            "SELECT to_jsonb(a) FROM decision_audit a
             JOIN refund_requests r ON r.id = a.refund_request_id
             WHERE r.conversation_id = $1::uuid",
        )
        .bind(conversation)
        .fetch_one(&self.pool)
        .await
        .expect("conversation has a decision audit")
    }
}

pub fn order_id(order_ref: &str) -> Uuid {
    db::seed::stable_id("order", order_ref)
}

/// What a correct intake returns for the first item of a seeded order.
pub fn complete_intake(order_ref: &str, reason: ReasonCategory) -> IntakeOutput {
    IntakeOutput {
        intent: Intent::RefundRequest,
        status: IntakeStatus::Complete,
        missing: vec![],
        order_id: Some(order_id(order_ref)),
        order_item_id: Some(db::seed::item_id(order_ref, 0)),
        mentioned_order_refs: vec![order_ref.to_owned()],
        reason_category: Some(reason),
        claimed_amount_cents: None,
        contradictory_statements: false,
        injection_signals: vec![],
        confidence: 0.95,
    }
}

pub fn needs_info_intake(missing: Vec<MissingField>) -> IntakeOutput {
    IntakeOutput {
        intent: Intent::RefundRequest,
        status: IntakeStatus::NeedsInfo,
        missing,
        order_id: None,
        order_item_id: None,
        mentioned_order_refs: vec![],
        reason_category: None,
        claimed_amount_cents: None,
        contradictory_statements: false,
        injection_signals: vec![],
        confidence: 0.9,
    }
}

/// `FakeAssistant` with one gate: the first verdict wording waits until
/// `release`, so a test can run a second chat while the first sits between
/// deciding and writing its request. Final questions and other replies pass.
pub struct GatedAssistant {
    pub fake: Arc<FakeAssistant>,
    armed: AtomicBool,
    entered: Notify,
    released: Notify,
}

impl GatedAssistant {
    pub fn new(fake: Arc<FakeAssistant>) -> Arc<GatedAssistant> {
        Arc::new(GatedAssistant {
            fake,
            armed: AtomicBool::new(false),
            entered: Notify::new(),
            released: Notify::new(),
        })
    }

    /// Holds the next verdict wording.
    pub fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Resolves once a held run has decided and is waiting.
    pub async fn entered(&self) {
        self.entered.notified().await;
    }

    pub fn release(&self) {
        self.released.notify_one();
    }
}

#[async_trait]
impl RefundAssistant for GatedAssistant {
    async fn intake(
        &self,
        input: &IntakeInput,
        model: &StageModel,
    ) -> Result<Completed<IntakeOutput>, AiError> {
        self.fake.intake(input, model).await
    }

    async fn respond(
        &self,
        input: &ResponderInput,
        model: &StageModel,
    ) -> Result<Completed<String>, AiError> {
        if matches!(input, ResponderInput::Verdict { .. })
            && self.armed.swap(false, Ordering::SeqCst)
        {
            self.entered.notify_one();
            self.released.notified().await;
        }
        self.fake.respond(input, model).await
    }

    async fn review(
        &self,
        input: &ReviewInput,
        model: &StageModel,
    ) -> Result<Completed<ReviewOutput>, AiError> {
        self.fake.review(input, model).await
    }

    async fn notice(
        &self,
        input: &NoticeInput,
        model: &StageModel,
    ) -> Result<Completed<NoticeOutput>, AiError> {
        self.fake.notice(input, model).await
    }
}
