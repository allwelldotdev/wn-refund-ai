//! Test orders customers add from My orders (demo only).

mod common;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use common::{TestApp, complete_intake};
use domain::types::ReasonCategory;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn today_minus(days: i64) -> String {
    (Utc::now().date_naive() - Duration::days(days)).to_string()
}

fn used_booking() -> Value {
    json!({
        "placed_on": today_minus(2),
        "items": [{ "item": "meeting-room", "quantity": 3 }, { "item": "coffee-add-on", "quantity": 1 }],
        "fulfilment": "used"
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_customer_adds_a_test_order_with_the_next_number(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    let catalog = app.get("/api/catalog", &amara).await.json();
    assert_eq!(catalog["next_order_ref"], "ORD-10438");
    let groups: Vec<&str> = catalog["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["label"].as_str().unwrap())
        .collect();
    assert_eq!(
        groups,
        [
            "Workspace & bookings",
            "Add-ons & services",
            "Accessories & products"
        ]
    );

    let res = app.post("/api/orders", &amara, used_booking()).await;
    assert_eq!(res.status, StatusCode::OK);
    let order = res.json();
    assert_eq!(order["ref"], "ORD-10438");
    assert_eq!(order["is_test"], true);
    assert_eq!(order["fulfilment"], "used");
    assert_eq!(order["total_cents"], 8000);
    assert_eq!(order["delivered_at"], order["placed_at"]);
    let names: Vec<&str> = order["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Coffee add-on", "Meeting Room (3 hrs)"]);

    let mine = app.get("/api/orders", &amara).await.json();
    assert_eq!(mine[0]["ref"], "ORD-10438");
    let sofia = app.login("sofia.rossi@example.com").await;
    let theirs = app.get("/api/orders", &sofia).await.json();
    assert!(
        theirs
            .as_array()
            .unwrap()
            .iter()
            .all(|o| o["ref"] != "ORD-10438")
    );
    let next = app.get("/api/catalog", &sofia).await.json();
    assert_eq!(next["next_order_ref"], "ORD-10439");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_added_order_goes_through_the_refund_chat(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    let order = app.post("/api/orders", &amara, used_booking()).await.json();
    let order_id: Uuid = order["id"].as_str().unwrap().parse().unwrap();
    let room = order["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "Meeting Room (3 hrs)")
        .unwrap();

    let conv = app.new_conversation(&amara).await;
    let mut intake = complete_intake("ORD-10437", ReasonCategory::Damaged);
    intake.order_id = Some(order_id);
    intake.order_item_id = Some(room["id"].as_str().unwrap().parse().unwrap());
    intake.mentioned_order_refs = vec!["ORD-10438".into()];
    app.fake.push_intake(Ok(intake));
    let res = app
        .say_with(
            &amara,
            &conv,
            "The meeting room projector was broken the whole time.",
            Some(order_id),
            Uuid::new_v4(),
        )
        .await;
    let request = res.event("request_updated");
    assert_eq!(request["state"], "approved");
    assert_eq!(request["order_ref"], "ORD-10438");
    assert_eq!(request["amount_cents"], 7200);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_preview_runs_the_real_policy_per_item(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    let body = json!({
        "placed_on": today_minus(1),
        "items": [
            { "item": "desk-lamp", "quantity": 1 },
            { "item": "hoodie-clearance", "quantity": 1 },
            { "item": "private-office", "quantity": 1 }
        ],
        "fulfilment": "delivered",
        "delivered_on": today_minus(0)
    });
    let res = app.post("/api/orders/preview", &amara, body).await;
    assert_eq!(res.status, StatusCode::OK);
    let p = res.json();
    assert!(p["assumption"].as_str().unwrap().contains("damaged"));
    let verdicts: Vec<(&str, &str)> = p["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["name"].as_str().unwrap(), i["verdict"].as_str().unwrap()))
        .collect();
    assert_eq!(
        verdicts,
        [
            ("Worknoon Desk Lamp", "approved"),
            ("Worknoon Hoodie (clearance)", "denied"),
            ("Private Office, monthly deposit", "escalated"),
        ]
    );
    assert!(
        p["items"][1]["reason"]
            .as_str()
            .unwrap()
            .contains("final sale")
    );

    // Nothing was stored.
    let catalog = app.get("/api/catalog", &amara).await.json();
    assert_eq!(catalog["next_order_ref"], "ORD-10438");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_bad_order_lists_every_problem(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    let res = app
        .post(
            "/api/orders",
            &amara,
            json!({
                "placed_on": today_minus(61),
                "items": [{ "item": "made-up", "quantity": 1 }],
                "fulfilment": null
            }),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let err = res.json();
    let paths: Vec<&str> = err["error"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["placed_on", "items[0].item", "fulfilment"]);

    let admin = app.login("ngozi.adeyemi@worknoon.example").await;
    let res = app.post("/api/orders", &admin, used_booking()).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}
