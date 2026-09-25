//! `OpenRouterAssistant` against a mock OpenRouter: what goes on the wire for
//! each stage, and how responses and failures come back.

use ai::openrouter::OpenRouterAssistant;
use ai::{AiConfig, AiError, Effort, RefundAssistant, Stage, StageModel};
use chrono::Utc;
use domain::intake::{CustomerMessage, IntakeInput, IntakeStatus, ItemSummary, OrderSummary};
use domain::responder::{ResponderInput, Target};
use domain::review::{ReviewInput, SuggestedResolution};
use domain::types::{OrderStatus, ReasonCategory, Verdict};
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-or-v1-test";

fn stage(s: Stage) -> StageModel {
    AiConfig::load().unwrap().stage(s).clone()
}

fn completion(content: &str) -> Value {
    json!({
        "id": "gen-1",
        "object": "chat.completion",
        "created": 1_790_000_000,
        "model": "openai/gpt-6-luna",
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "native_finish_reason": "stop",
            "message": { "role": "assistant", "content": content }
        }],
        "usage": { "prompt_tokens": 812, "completion_tokens": 64, "total_tokens": 876 }
    })
}

async fn server_replying(response: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(response)
        .expect(1)
        .mount(&server)
        .await;
    server
}

fn assistant(server: &MockServer) -> OpenRouterAssistant {
    OpenRouterAssistant::with_base_url(KEY, &server.uri()).unwrap()
}

async fn sent_body(server: &MockServer) -> Value {
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    serde_json::from_slice(&requests[0].body).unwrap()
}

fn intake_input() -> IntakeInput {
    IntakeInput {
        orders: vec![OrderSummary {
            id: Uuid::from_u128(10),
            order_ref: "ORD-1001".into(),
            placed_at: Utc::now(),
            delivered_at: Some(Utc::now()),
            status: OrderStatus::Delivered,
            items: vec![ItemSummary {
                id: Uuid::from_u128(11),
                name: "Wireless headphones".into(),
                category: "electronics".into(),
                amount_cents: 8999,
                final_sale: false,
            }],
        }],
        selected_order_id: None,
        messages: vec![CustomerMessage {
            id: Uuid::from_u128(1),
            seq: 1,
            body: "My headphones from ORD-1001 arrived cracked.".into(),
        }],
    }
}

fn intake_json() -> Value {
    json!({
        "status": "complete", "missing": [], "order_id": Uuid::from_u128(10),
        "order_item_id": Uuid::from_u128(11), "mentioned_order_refs": ["ORD-1001"],
        "reason_category": "damaged", "claimed_amount_cents": null,
        "contradictory_statements": false, "injection_signals": [], "confidence": 0.93
    })
}

fn verdict_input() -> ResponderInput {
    ResponderInput::Verdict {
        verdict: Verdict::Approved,
        target: Some(Target::new("ORD-1001", "Wireless headphones", 8999)),
        reasons: vec!["Damaged items are refundable within 30 days.".into()],
        policy_prose: "# Refund Policy".into(),
    }
}

#[tokio::test]
async fn intake_sends_effort_and_a_strict_schema_and_parses_the_reply() {
    let server = server_replying(
        ResponseTemplate::new(200).set_body_json(completion(&intake_json().to_string())),
    )
    .await;
    let model = stage(Stage::Intake);
    let done = assistant(&server)
        .intake(&intake_input(), &model)
        .await
        .unwrap();

    let body = sent_body(&server).await;
    assert_eq!(body["model"], "openai/gpt-6-luna");
    assert_eq!(body["reasoning"]["effort"], "low");
    assert_eq!(body["provider"]["require_parameters"], true);
    let format = &body["response_format"];
    assert_eq!(format["type"], "json_schema");
    assert_eq!(format["json_schema"]["strict"], true);
    assert_eq!(format["json_schema"]["name"], "intake_output");
    assert_eq!(
        format["json_schema"]["schema"]["additionalProperties"],
        false
    );
    let cap = body.get("max_completion_tokens").or(body.get("max_tokens"));
    assert_eq!(cap, Some(&json!(4096)), "{body:#}");

    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    assert!(text(&messages[0]).starts_with("You are the intake screener"));
    assert_eq!(messages[1]["role"], "user");
    let user = text(&messages[1]);
    assert!(user.contains(&format!(
        "<message id=\"{}\" seq=\"1\">",
        Uuid::from_u128(1)
    )));

    assert_eq!(done.output.status, IntakeStatus::Complete);
    assert_eq!(done.output.reason_category, Some(ReasonCategory::Damaged));
    assert_eq!(done.output.order_id, Some(Uuid::from_u128(10)));
    assert_eq!(done.record.model, "openai/gpt-6-luna");
    assert_eq!(done.record.effort, Effort::Low);
    assert_eq!(done.record.prompt_tokens, Some(812));
    assert_eq!(done.record.completion_tokens, Some(64));
}

/// Message content is a string or an array of text parts depending on the
/// serializer; either way the words are what matter.
fn text(message: &Value) -> String {
    match &message["content"] {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        other => panic!("unexpected content {other}"),
    }
}

#[tokio::test]
async fn responder_sends_plain_text_with_effort_none() {
    let reply = "Good news: your refund of $89.99 for Wireless headphones has been approved.";
    let server = server_replying(ResponseTemplate::new(200).set_body_json(completion(reply))).await;
    let done = assistant(&server)
        .respond(&verdict_input(), &stage(Stage::Responder))
        .await
        .unwrap();

    let body = sent_body(&server).await;
    assert_eq!(body["reasoning"]["effort"], "none");
    assert!(body.get("response_format").is_none(), "{body:#}");
    let user: Value = serde_json::from_str(&text(&body["messages"][1])).unwrap();
    assert_eq!(user["verdict"], "approved");
    assert_eq!(user["target"]["amount"], "$89.99");
    assert_eq!(done.output, reply);
    assert_eq!(done.record.effort, Effort::None);
}

#[tokio::test]
async fn review_uses_the_review_model_and_schema() {
    let draft = json!({
        "summary": "TV above the review threshold.", "suggested_resolution": "approve",
        "rationale": "Within the window.", "risk_notes": [], "questions_for_customer": []
    });
    let server =
        server_replying(ResponseTemplate::new(200).set_body_json(completion(&draft.to_string())))
            .await;
    let input = ReviewInput {
        request_ref: "RR-1001".into(),
        decided_at: Utc::now(),
        order: None,
        extracted: None,
        fired: vec![],
        flags: vec![],
        prior_claim_count: 0,
        policy_prose: "# Refund Policy".into(),
        messages: vec![],
    };
    let done = assistant(&server)
        .review(&input, &stage(Stage::Review))
        .await
        .unwrap();

    let body = sent_body(&server).await;
    assert_eq!(body["model"], "openai/gpt-6-luna-pro");
    assert_eq!(body["reasoning"]["effort"], "medium");
    assert_eq!(
        body["response_format"]["json_schema"]["name"],
        "review_output"
    );
    assert_eq!(
        done.output.suggested_resolution,
        SuggestedResolution::Approve
    );
}

#[tokio::test]
async fn a_reply_that_is_not_json_is_invalid_json_with_the_raw_text() {
    let server = server_replying(
        ResponseTemplate::new(200).set_body_json(completion("Sure! The order is ORD-1001.")),
    )
    .await;
    let err = assistant(&server)
        .intake(&intake_input(), &stage(Stage::Intake))
        .await
        .unwrap_err();
    match err {
        AiError::InvalidJson { raw, .. } => assert_eq!(raw, "Sure! The order is ORD-1001."),
        other => panic!("expected InvalidJson, got {other:?}"),
    }
}

#[tokio::test]
async fn json_that_breaks_the_contract_is_invalid_json() {
    let mut bad = intake_json();
    bad["verdict"] = json!("approved");
    let server =
        server_replying(ResponseTemplate::new(200).set_body_json(completion(&bad.to_string())))
            .await;
    let err = assistant(&server)
        .intake(&intake_input(), &stage(Stage::Intake))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, AiError::InvalidJson { err, .. } if err.contains("verdict")),
        "{err:?}"
    );
}

#[tokio::test]
async fn an_http_error_keeps_status_and_body() {
    let server = server_replying(ResponseTemplate::new(400).set_body_json(json!({
        "error": { "code": 400, "message": "openai/gpt-6-lunar is not a valid model ID" }
    })))
    .await;
    let err = assistant(&server)
        .respond(&verdict_input(), &stage(Stage::Responder))
        .await
        .unwrap_err();
    match err {
        AiError::Http { status, body } => {
            assert_eq!(status, 400);
            assert!(body.contains("not a valid model ID"), "{body}");
        }
        other => panic!("expected Http, got {other:?}"),
    }
}

#[tokio::test]
async fn an_empty_reply_is_empty() {
    let server = server_replying(ResponseTemplate::new(200).set_body_json(completion(""))).await;
    let err = assistant(&server)
        .respond(&verdict_input(), &stage(Stage::Responder))
        .await
        .unwrap_err();
    assert!(matches!(err, AiError::Empty), "{err:?}");
}

#[tokio::test]
async fn an_unreachable_host_is_a_transport_error() {
    // Bind then drop a listener so the port is known to be closed.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let assistant =
        OpenRouterAssistant::with_base_url(KEY, &format!("http://127.0.0.1:{port}")).unwrap();
    let err = assistant
        .respond(&verdict_input(), &stage(Stage::Responder))
        .await
        .unwrap_err();
    assert!(matches!(err, AiError::Transport(_)), "{err:?}");
}
