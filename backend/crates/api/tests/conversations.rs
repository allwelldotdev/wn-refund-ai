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
    let alice = app.login("alice@example.com").await;
    let orders = app.get("/api/orders", &alice).await.json();
    let refs: Vec<&str> = orders
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["ref"].as_str().unwrap())
        .collect();
    assert_eq!(refs, ["ORD-1001", "ORD-1002"]);
    let headphones = &orders[0]["items"][0];
    assert_eq!(headphones["name"], "Wireless headphones");
    assert_eq!(headphones["amount_cents"], 8999);
    assert_eq!(headphones["active_refund"], false);
    assert_eq!(orders[0]["status"], "delivered");
    assert_eq!(orders[0]["total_cents"], 8999);

    let ivan = app.login("ivan@example.com").await;
    let orders = app.get("/api/orders", &ivan).await.json();
    assert_eq!(orders[0]["items"][0]["active_refund"], true);

    let marco = app.login("marco@example.com").await;
    let orders = app.get("/api/orders", &marco).await.json();
    let items = &orders[0]["items"];
    assert_eq!(find(items, "name", "Wool scarf")["final_sale"], true);
    assert_eq!(find(items, "name", "Table lamp")["final_sale"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn customers_create_list_and_read_their_conversations(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let alice = app.login("alice@example.com").await;
    assert_eq!(
        app.get("/api/conversations", &alice).await.json(),
        json!([])
    );

    let res = app.post("/api/conversations", &alice, json!({})).await;
    assert_eq!(res.status, StatusCode::CREATED);
    let id = res.json()["id"].as_str().unwrap().to_owned();

    let list = app.get("/api/conversations", &alice).await.json();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["id"], id.as_str());
    assert_eq!(list[0]["last_seq"], 0);
    assert_eq!(list[0]["request"], Value::Null);

    let res = app.get(&format!("/api/conversations/{id}"), &alice).await;
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
    let alice = app.login("alice@example.com").await;
    let id = app
        .post("/api/conversations", &alice, json!({}))
        .await
        .json()["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let ben = app.login("ben@example.com").await;
    let res = app.get(&format!("/api/conversations/{id}"), &ben).await;
    assert_eq!(
        (res.status, res.error_code().as_str()),
        (StatusCode::NOT_FOUND, "not_found")
    );
    assert_eq!(app.get("/api/conversations", &ben).await.json(), json!([]));

    let admin = app.login("admin@example.com").await;
    for path in ["/api/orders", "/api/conversations"] {
        let res = app.get(path, &admin).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(res.error_code(), "customer_only");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn seeded_history_shows_as_resolved_conversations(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let farah = app.login("farah@example.com").await;
    let list = app.get("/api/conversations", &farah).await.json();
    assert_eq!(list.as_array().unwrap().len(), 2);
    let lamp = find(
        &list,
        "preview",
        "My desk lamp arrived with a cracked base.",
    );
    assert_eq!(lamp["request"]["ref"], "RR-0901");
    assert_eq!(lamp["request"]["state"], "resolved_approved");
    assert_eq!(lamp["request"]["order_ref"], "ORD-1007");
    assert_eq!(lamp["request"]["item_name"], "Desk lamp");

    let id = lamp["id"].as_str().unwrap();
    let body = app
        .get(&format!("/api/conversations/{id}"), &farah)
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
