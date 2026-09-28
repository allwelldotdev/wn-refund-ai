//! Admins messaging the customer on an escalated request, and the read
//! markers behind both sides' unread counts.

mod common;

use axum::http::StatusCode;
use common::{TestApp, complete_intake};
use domain::types::ReasonCategory;
use serde_json::{Value, json};
use sqlx::PgPool;

/// Grace's $1,200 office deposit: escalated. Returns (customer token,
/// conversation, request ref, admin token).
async fn escalated(app: &TestApp) -> (String, String, String, String) {
    let grace = app.login("grace.liu@example.com").await;
    let conv = app.new_conversation(&grace).await;
    let res = app
        .decide(
            &grace,
            &conv,
            "I'm relocating, so please cancel ORD-10388 and return the deposit.",
            None,
            complete_intake("ORD-10388", ReasonCategory::ChangedMind),
        )
        .await;
    assert_eq!(res.event("request_updated")["state"], "escalated");
    let request_ref = res.event("request_updated")["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let admin = app.login("ngozi.adeyemi@worknoon.example").await;
    (grace, conv, request_ref, admin)
}

fn messages_path(request_ref: &str) -> String {
    format!("/api/admin/requests/{request_ref}/messages")
}

async fn summary(app: &TestApp, token: &str, conv: &str) -> Value {
    let list = app.get("/api/conversations", token).await.json();
    list.as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == conv)
        .unwrap()
        .clone()
}

async fn queue_row(app: &TestApp, admin: &str, request_ref: &str) -> Value {
    let list = app
        .get("/api/admin/requests?state=escalated", admin)
        .await
        .json();
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["ref"] == request_ref)
        .unwrap()
        .clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_admin_message_reaches_the_customer_as_written(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (grace, conv, request_ref, admin) = escalated(&app).await;
    let text = "  Could you send the relocation letter?\nA photo is fine.  ";

    let res = app
        .post(
            &messages_path(&request_ref),
            &admin,
            json!({ "body": text }),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let sent = res.json();
    assert_eq!(sent["role"], "admin");
    assert_eq!(
        sent["body"],
        "Could you send the relocation letter?\nA photo is fine."
    );
    assert_eq!(sent["author_name"], "Ngozi Adeyemi");

    let got = app
        .get(&format!("/api/conversations/{conv}"), &grace)
        .await
        .json();
    let last = got["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["id"], sent["id"]);
    assert_eq!(last["author_name"], "Ngozi Adeyemi");

    // Unread for the customer until they have seen it.
    let s = summary(&app, &grace, &conv).await;
    assert_eq!(s["unread_count"], 1);
    assert_eq!(s["last_reply_by"], "Ngozi Adeyemi");
    assert!(s["last_reply_at"].is_string());
    let res = app
        .post(
            &format!("/api/conversations/{conv}/read"),
            &grace,
            json!({ "seq": sent["seq"] }),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert_eq!(summary(&app, &grace, &conv).await["unread_count"], 0);

    // The case file shows who wrote it.
    let d = app
        .get(&format!("/api/admin/requests/{request_ref}"), &admin)
        .await
        .json();
    let admin_msg = d["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "admin")
        .unwrap()
        .clone();
    assert_eq!(admin_msg["author_name"], "Ngozi Adeyemi");
}

#[sqlx::test(migrations = "../../migrations")]
async fn customer_replies_are_unread_for_admins_and_get_no_bot_reply(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (grace, conv, request_ref, admin) = escalated(&app).await;
    let row = queue_row(&app, &admin, &request_ref).await;
    assert_eq!(row["unread_from_customer"], 0);
    assert_eq!(row["customer_replied_at"], Value::Null);

    // Before any admin has written: one holding reply, then quiet.
    let res = app
        .say(&grace, &conv, "I have the relocation letter.")
        .await;
    assert_eq!(res.event("reply_start")["kind"], "holding");
    let res = app.say(&grace, &conv, "It's dated September 20.").await;
    assert_eq!(res.event_names(), ["message_saved", "done"]);
    let row = queue_row(&app, &admin, &request_ref).await;
    assert_eq!(row["unread_from_customer"], 2);
    assert!(row["customer_replied_at"].is_string());

    // The admin reads, then replies: nothing unread, nothing awaiting a reply.
    let seq = app
        .get(&format!("/api/admin/requests/{request_ref}"), &admin)
        .await
        .json()["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["seq"]
        .clone();
    let res = app
        .post(
            &format!("/api/admin/requests/{request_ref}/read"),
            &admin,
            json!({ "seq": seq }),
        )
        .await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert_eq!(
        queue_row(&app, &admin, &request_ref).await["unread_from_customer"],
        0
    );
    app.post(
        &messages_path(&request_ref),
        &admin,
        json!({ "body": "Thanks, please upload it here." }),
    )
    .await;
    assert_eq!(
        queue_row(&app, &admin, &request_ref).await["customer_replied_at"],
        Value::Null
    );

    // Once an admin has written, the assistant stays out of it.
    let res = app.say(&grace, &conv, "Sent it by email.").await;
    assert_eq!(res.event_names(), ["message_saved", "done"]);
    let row = queue_row(&app, &admin, &request_ref).await;
    assert_eq!(row["unread_from_customer"], 1);
    assert!(row["customer_replied_at"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_decision_message_names_its_author_and_closes_the_chat(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (grace, conv, request_ref, admin) = escalated(&app).await;
    let res = app
        .resolve(
            &admin,
            &request_ref,
            "approved",
            "Relocation letter checked; deposit returned in full.",
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let s = summary(&app, &grace, &conv).await;
    assert_eq!(s["unread_count"], 1);
    assert_eq!(s["last_reply_by"], "Ngozi Adeyemi");

    let res = app
        .post(
            &messages_path(&request_ref),
            &admin,
            json!({ "body": "One more thing." }),
        )
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    assert_eq!(res.error_code(), "not_escalated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_message_guards(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (grace, conv, request_ref, admin) = escalated(&app).await;
    for body in ["   ", &"x".repeat(4001)] {
        let res = app
            .post(
                &messages_path(&request_ref),
                &admin,
                json!({ "body": body }),
            )
            .await;
        assert_eq!(
            res.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            body.len()
        );
    }
    let res = app
        .post(&messages_path("RR-9999"), &admin, json!({ "body": "Hi" }))
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    // Customers can't post as an admin, or mark someone else's chat read.
    let res = app
        .post(
            &messages_path(&request_ref),
            &grace,
            json!({ "body": "Hi" }),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    let amara = app.login("amara.okafor@example.com").await;
    let res = app
        .post(
            &format!("/api/conversations/{conv}/read"),
            &amara,
            json!({ "seq": 1 }),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}
