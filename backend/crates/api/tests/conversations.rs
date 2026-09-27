mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};
use sqlx::PgPool;

fn find<'a>(items: &'a Value, key: &str, value: &str) -> &'a Value {
    items
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v[key] == value)
        .unwrap_or_else(|| panic!("no item with {key} = {value}"))
}

#[sqlx::test(migrations = "../../migrations")]
async fn orders_are_the_customers_own_with_refund_markers(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    let orders = app.get("/api/orders", &amara).await.json();
    let refs: Vec<&str> = orders
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["ref"].as_str().unwrap())
        .collect();
    assert_eq!(
        refs,
        [
            "ORD-10437",
            "ORD-10430",
            "ORD-10426",
            "ORD-10421",
            "ORD-10416",
            "ORD-10331"
        ]
    );
    let lamp = &orders[0]["items"][0];
    assert_eq!(lamp["name"], "Worknoon Desk Lamp");
    assert_eq!(lamp["amount_cents"], 6200);
    assert_eq!(lamp["active_refund"], false);
    assert_eq!(orders[0]["status"], "delivered");
    assert_eq!(orders[0]["total_cents"], 6200);
    assert_eq!(find(&orders, "ref", "ORD-10426")["status"], "processing");

    let fatima = app.login("fatima.bello@example.com").await;
    let orders = app.get("/api/orders", &fatima).await.json();
    assert_eq!(orders[0]["items"][0]["active_refund"], true);

    let rafael = app.login("rafael.costa@example.com").await;
    let orders = app.get("/api/orders", &rafael).await.json();
    let items = &orders[0]["items"];
    assert_eq!(
        find(items, "name", "Branded Tote Bag (clearance)")["final_sale"],
        true
    );
    assert_eq!(find(items, "name", "Laptop Stand")["final_sale"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn customers_create_list_and_read_their_conversations(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    // Amara starts with one seeded, resolved conversation (RR-0904).
    let seeded = app.get("/api/conversations", &amara).await.json();
    assert_eq!(seeded.as_array().unwrap().len(), 1);
    assert_eq!(seeded[0]["request"]["ref"], "RR-0904");

    let res = app.post("/api/conversations", &amara, json!({})).await;
    assert_eq!(res.status, StatusCode::CREATED);
    let id = res.json()["id"].as_str().unwrap().to_owned();

    let list = app.get("/api/conversations", &amara).await.json();
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[0]["last_seq"], 0);
    assert_eq!(list[0]["request"], Value::Null);

    let res = app.get(&format!("/api/conversations/{id}"), &amara).await;
    assert_eq!(res.status, StatusCode::OK);
    let body = res.json();
    assert_eq!(body["conversation"]["id"], id.as_str());
    assert_eq!(body["messages"], json!([]));
    assert_eq!(body["request"], Value::Null);
    assert!(body["conversation"].get("customer_id").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn other_customers_and_admins_cannot_read_a_conversation(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let amara = app.login("amara.okafor@example.com").await;
    let id = app
        .post("/api/conversations", &amara, json!({}))
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let sofia = app.login("sofia.rossi@example.com").await;
    let res = app.get(&format!("/api/conversations/{id}"), &sofia).await;
    assert_eq!(
        (res.status, res.error_code().as_str()),
        (StatusCode::NOT_FOUND, "not_found")
    );
    assert_eq!(
        app.get("/api/conversations", &sofia).await.json(),
        json!([])
    );

    let admin = app.login("ngozi.adeyemi@worknoon.example").await;
    for path in ["/api/orders", "/api/conversations"] {
        let res = app.get(path, &admin).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(res.error_code(), "customer_only");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn seeded_history_shows_as_resolved_conversations(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let chiamaka = app.login("chiamaka.eze@example.com").await;
    let list = app.get("/api/conversations", &chiamaka).await.json();
    assert_eq!(list.as_array().unwrap().len(), 2);
    let headset = find(
        &list,
        "preview",
        "The headset arrived with a broken ear cup.",
    );
    assert_eq!(headset["request"]["ref"], "RR-0901");
    assert_eq!(headset["request"]["state"], "resolved_approved");
    assert_eq!(headset["request"]["order_ref"], "ORD-10288");
    assert_eq!(headset["request"]["item_name"], "Noise-cancelling Headset");

    let id = headset["id"].as_str().unwrap();
    let body = app
        .get(&format!("/api/conversations/{id}"), &chiamaka)
        .await
        .json();
    let roles: Vec<(&str, &Value)> = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["role"].as_str().unwrap(), &m["assistant_kind"]))
        .collect();
    assert_eq!(
        roles,
        [("customer", &Value::Null), ("assistant", &json!("verdict"))]
    );
    assert_eq!(body["request"]["ref"], "RR-0901");
}
