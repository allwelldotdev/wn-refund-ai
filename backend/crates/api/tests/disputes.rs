//! Disputes of automatic denials and the admin setting that allows them.

mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{TestApp, complete_intake};
use domain::types::ReasonCategory;
use serde_json::{Value, json};
use sqlx::PgPool;

/// Tomás's final-sale locker: denied automatically. Returns (token, conversation).
async fn denied_locker(app: &TestApp) -> (String, String) {
    let tomas = app.login("tomas.herrera@example.com").await;
    let conv = app.new_conversation(&tomas).await;
    app.fake
        .push_intake(Ok(complete_intake("ORD-10397", ReasonCategory::Damaged)));
    let res = app
        .say(&tomas, &conv, "The lock on my ORD-10397 locker is broken.")
        .await;
    assert_eq!(res.event("request_updated")["state"], "denied");
    assert_eq!(res.event("request_updated")["can_dispute"], true);
    (tomas, conv)
}

fn dispute_path(conv: &str) -> String {
    format!("/api/conversations/{conv}/dispute")
}

async fn review_status(app: &TestApp, conv: &str) -> String {
    let mut status = String::new();
    for _ in 0..100 {
        status = sqlx::query_scalar(
            "SELECT v.status FROM escalation_reviews v
             JOIN refund_requests r ON r.id = v.refund_request_id
             WHERE r.conversation_id = $1::uuid",
        )
        .bind(conv)
        .fetch_one(&app.pool)
        .await
        .unwrap();
        if status != "pending" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    status
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_automatic_denial_can_be_disputed_once(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (tomas, conv) = denied_locker(&app).await;

    let res = app
        .post(
            &dispute_path(&conv),
            &tomas,
            json!({ "reason": "The checkout never said final sale." }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let summary = res.json();
    assert_eq!(summary["state"], "escalated");
    assert_eq!(summary["can_dispute"], false);
    assert!(summary["disputed_at"].is_string());

    let got = app
        .get(&format!("/api/conversations/{conv}"), &tomas)
        .await
        .json();
    let tail: Vec<(&str, &str)> = got["messages"].as_array().unwrap()[2..]
        .iter()
        .map(|m| (m["role"].as_str().unwrap(), m["body"].as_str().unwrap()))
        .collect();
    assert_eq!(tail[0], ("customer", "The checkout never said final sale."));
    assert_eq!(tail[1].0, "system");
    assert!(tail[1].1.starts_with("You disputed this decision on "));
    assert_eq!(review_status(&app, &conv).await, "drafted");

    // Once only; the original decision stays on record.
    let again = app.post(&dispute_path(&conv), &tomas, json!({})).await;
    assert_eq!(again.status, StatusCode::CONFLICT);
    assert_eq!(again.error_code(), "already_disputed");
    assert_eq!(app.audit(&conv).await["verdict"], "denied");

    let admin = app.login("ngozi.adeyemi@worknoon.example").await;
    let list = app
        .get("/api/admin/requests?disputed=true", &admin)
        .await
        .json();
    assert_eq!(list["total"], 1);
    let item = &list["items"][0];
    assert_eq!(item["state"], "escalated");
    assert!(item["disputed_at"].is_string());
    let detail = app
        .get(
            &format!("/api/admin/requests/{}", item["ref"].as_str().unwrap()),
            &admin,
        )
        .await
        .json();
    let kinds: Vec<&str> = detail["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["decided", "disputed", "review_drafted"]);
    assert_eq!(detail["timeline"][1]["actor_kind"], "customer");
    assert!(detail["request"]["disputed_at"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_dispute_without_a_reason_adds_only_the_note(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (tomas, conv) = denied_locker(&app).await;
    let res = app
        .post(&dispute_path(&conv), &tomas, json!({ "reason": "  " }))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let got = app
        .get(&format!("/api/conversations/{conv}"), &tomas)
        .await
        .json();
    let roles: Vec<&str> = got["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["customer", "assistant", "system"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn turning_disputes_off_makes_automatic_denials_final(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let (tomas, conv) = denied_locker(&app).await;
    let admin = app.login("ngozi.adeyemi@worknoon.example").await;

    let initial = app.get("/api/admin/settings", &admin).await.json();
    assert_eq!(
        initial,
        json!({ "allow_disputes": true, "updated_by": null, "updated_at": null })
    );
    let off = app
        .call(
            Method::PUT,
            "/api/admin/settings",
            Some(&admin),
            Some(&json!({ "allow_disputes": false })),
        )
        .await
        .json();
    assert_eq!(off["allow_disputes"], false);
    assert_eq!(off["updated_by"], "Ngozi Adeyemi");

    let conversations = app.get("/api/conversations", &tomas).await.json();
    let request = conversations
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == conv.as_str())
        .unwrap()["request"]
        .clone();
    assert_eq!(request["can_dispute"], false);
    let res = app.post(&dispute_path(&conv), &tomas, json!({})).await;
    assert_eq!(res.error_code(), "disputes_off");

    // Saving the same value again does not change who changed it last.
    let sam = app.login("sam.whitfield@worknoon.example").await;
    let same = app
        .call(
            Method::PUT,
            "/api/admin/settings",
            Some(&sam),
            Some(&json!({ "allow_disputes": false })),
        )
        .await
        .json();
    assert_eq!(same["updated_by"], "Ngozi Adeyemi");

    let tomas_settings = app.get("/api/admin/settings", &tomas).await;
    assert_eq!(tomas_settings.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_an_automatic_denial_can_be_disputed(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    let approved = app.new_conversation(&amara).await;
    app.fake
        .push_intake(Ok(complete_intake("ORD-10437", ReasonCategory::Damaged)));
    app.say(&amara, &approved, "My desk lamp arrived broken.")
        .await;
    let res = app.post(&dispute_path(&approved), &amara, json!({})).await;
    assert_eq!(res.error_code(), "not_disputable");

    // Someone else's conversation is not found at all.
    let (_, conv) = denied_locker(&app).await;
    let res = app.post(&dispute_path(&conv), &amara, json!({})).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);

    // A denial made by an admin is final.
    let grace = app.login("grace.liu@example.com").await;
    let escalated = app.new_conversation(&grace).await;
    app.fake.push_intake(Ok(complete_intake(
        "ORD-10388",
        ReasonCategory::ChangedMind,
    )));
    let res = app
        .say(
            &grace,
            &escalated,
            "Please cancel ORD-10388 and refund the deposit.",
        )
        .await;
    let request_ref = res.event("request_updated")["ref"]
        .as_str()
        .unwrap()
        .to_owned();
    let admin = app.login("ngozi.adeyemi@worknoon.example").await;
    let resolved = app
        .post(
            &format!("/api/admin/requests/{request_ref}/resolve"),
            &admin,
            json!({ "resolution": "denied", "note": "The deposit is non-refundable now." }),
        )
        .await;
    assert_eq!(resolved.status, StatusCode::OK);
    let res = app.post(&dispute_path(&escalated), &grace, json!({})).await;
    assert_eq!(res.error_code(), "not_disputable");
    let got: Value = app
        .get(&format!("/api/conversations/{escalated}"), &grace)
        .await
        .json();
    assert_eq!(got["request"]["can_dispute"], false);
}
