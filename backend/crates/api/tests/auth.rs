mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../migrations")]
async fn health_and_unknown_routes(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let res = app.call(Method::GET, "/api/health", None, None).await;
    assert_eq!(
        (res.status, res.json()),
        (StatusCode::OK, json!({ "status": "ok" }))
    );

    let res = app.call(Method::GET, "/api/nope", None, None).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert_eq!(res.error_code(), "not_found");
}

#[sqlx::test(migrations = "../../migrations")]
async fn login_returns_a_token_and_principal(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let body = json!({ "email": "  Alice@Example.com ", "password": "demo1234" });
    let res = app
        .call(Method::POST, "/api/auth/login", None, Some(&body))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let json = res.json();
    assert_eq!(json["principal"]["kind"], "customer");
    assert_eq!(json["principal"]["name"], "Alice Nguyen");
    let token = json["token"].as_str().unwrap();

    let me = app.get("/api/auth/me", token).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["principal"]["email"], "alice@example.com");

    let admin = app.login("admin@example.com").await;
    assert_eq!(
        app.get("/api/auth/me", &admin).await.json()["principal"]["kind"],
        "admin"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_credentials_are_rejected_alike(pool: PgPool) {
    let app = TestApp::new(pool).await;
    for body in [
        json!({ "email": "alice@example.com", "password": "wrong" }),
        json!({ "email": "nobody@example.com", "password": "demo1234" }),
    ] {
        let res = app
            .call(Method::POST, "/api/auth/login", None, Some(&body))
            .await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
        assert_eq!(res.error_code(), "invalid_credentials");
    }
    let res = app
        .call(
            Method::POST,
            "/api/auth/login",
            None,
            Some(&json!({ "email": "x" })),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(res.error_code(), "invalid_body");
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_malformed_and_unknown_tokens_are_401(pool: PgPool) {
    let app = TestApp::new(pool).await;
    assert_eq!(
        app.call(Method::GET, "/api/auth/me", None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    for token in ["not-a-uuid", "00000000-0000-0000-0000-000000000000"] {
        let res = app.get("/api/auth/me", token).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{token}");
        assert_eq!(res.error_code(), "unauthorized");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_invalidates_the_session(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let token = app.login("ben@example.com").await;
    let other = app.login("ben@example.com").await;
    let res = app.post("/api/auth/logout", &token, json!({})).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        app.get("/api/auth/me", &token).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(app.get("/api/auth/me", &other).await.status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_sessions_are_rejected(pool: PgPool) {
    let app = TestApp::new(pool.clone()).await;
    let token = app.login("chloe@example.com").await;
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.get("/api/auth/me", &token).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn demo_accounts_list_admins_then_customers(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let res = app
        .call(Method::GET, "/api/auth/demo-accounts", None, None)
        .await;
    let accounts = res.json();
    let accounts = accounts.as_array().unwrap();
    assert_eq!(accounts.len(), 17);
    assert_eq!(accounts[0]["role"], "admin");
    assert_eq!(
        accounts[2],
        json!({ "name": "Alice Nguyen", "email": "alice@example.com", "role": "customer", "scenario": "clean_damaged" })
    );
    assert!(accounts.iter().all(|a| a.get("password_hash").is_none()));
}
