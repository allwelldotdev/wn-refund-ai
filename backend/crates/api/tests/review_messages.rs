//! Resolving an escalation tells the customer: a drafted "Dear …" message the
//! admin previews, and a one-line summary note, both posted to the chat.

mod common;

use ai::{AiError, Stage};
use axum::http::StatusCode;
use common::{TestApp, complete_intake};
use domain::notice::NoticeOutput;
use domain::types::ReasonCategory;
use serde_json::{Value, json};
use sqlx::PgPool;

/// Grace's $1,200 office deposit: escalated. Returns (conversation, ref).
async fn escalated(app: &TestApp) -> (String, String) {
    let grace = app.login("grace.liu@example.com").await;
    let conv = app.new_conversation(&grace).await;
    app.fake.push_intake(Ok(complete_intake(
        "ORD-10388",
        ReasonCategory::ChangedMind,
    )));
    let res = app
        .say(
            &grace,
            &conv,
            "I'm relocating, so please cancel ORD-10388 and return the deposit.",
        )
        .await;
    let request_ref = res.event("request_updated")["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    (conv, request_ref)
}

fn draft_path(request_ref: &str) -> String {
    format!("/api/admin/requests/{request_ref}/resolve/draft")
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_resolution_is_drafted_previewed_and_posted_to_the_chat(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (conv, request_ref) = escalated(&app).await;
    let admin = app.login("sam.whitfield@worknoon.example").await;
    let note =
        "Relocation letter checked; the office has not started, so the deposit is refundable.";

    let draft = app
        .post(
            &draft_path(&request_ref),
            &admin,
            json!({ "resolution": "approved", "note": note }),
        )
        .await;
    assert_eq!(draft.status, StatusCode::OK);
    let d = draft.json();
    assert!(d["message"].as_str().unwrap().starts_with("Dear Grace,"));
    assert_eq!(app.fake.calls_for(Stage::Notice).len(), 1);

    let res = app
        .post(
            &format!("/api/admin/requests/{request_ref}/resolve"),
            &admin,
            json!({ "resolution": "approved", "note": note,
                    "message": d["message"], "summary": d["summary"] }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);

    let grace = app.login("grace.liu@example.com").await;
    let got = app
        .get(&format!("/api/conversations/{conv}"), &grace)
        .await
        .json();
    let tail: Vec<(&str, &Value)> = got["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .take(2)
        .map(|m| (m["role"].as_str().unwrap(), &m["body"]))
        .collect();
    assert_eq!(tail[1], ("admin", &d["message"]));
    assert_eq!(tail[0], ("system", &d["summary"]));
    assert_eq!(got["request"]["state"], "resolved_approved");

    // The note stays in the audit exactly as the admin wrote it.
    let detail = app
        .get(&format!("/api/admin/requests/{request_ref}"), &admin)
        .await
        .json();
    assert_eq!(detail["review"]["resolution_note"], note);
}

#[sqlx::test(migrations = "../../migrations")]
async fn when_the_model_fails_the_review_is_not_completed(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (_, request_ref) = escalated(&app).await;
    let admin = app.login("sam.whitfield@worknoon.example").await;
    app.fake.push_notice(Err(AiError::Timeout(30)));
    // A notice that names the wrong outcome counts as a failed call too.
    app.fake.push_notice(Ok(NoticeOutput {
        message: "Dear Grace, your refund has been denied.".into(),
        summary: "A support specialist denied this refund.".into(),
    }));

    let res = app
        .post(
            &draft_path(&request_ref),
            &admin,
            json!({ "resolution": "approved", "note": "Deposit is refundable." }),
        )
        .await;
    assert_eq!(res.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(res.error_code(), "assistant_unavailable");
    assert_eq!(app.fake.calls_for(Stage::Notice).len(), 2);
    let detail = app
        .get(&format!("/api/admin/requests/{request_ref}"), &admin)
        .await
        .json();
    assert_eq!(detail["request"]["state"], "escalated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_message_that_contradicts_the_resolution_is_refused(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (conv, request_ref) = escalated(&app).await;
    let admin = app.login("sam.whitfield@worknoon.example").await;
    let res = app
        .post(
            &format!("/api/admin/requests/{request_ref}/resolve"),
            &admin,
            json!({
                "resolution": "denied", "note": "The deposit is non-refundable now.",
                "message": "Dear Grace, good news: your refund is approved.",
                "summary": "A support specialist approved this refund."
            }),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.json()["error"]["fields"][0]["path"], "message");
    let grace = app.login("grace.liu@example.com").await;
    let got = app
        .get(&format!("/api/conversations/{conv}"), &grace)
        .await
        .json();
    assert_eq!(got["request"]["state"], "escalated");
}
