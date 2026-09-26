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
    let body = json!({ "email": "  Amara.Okafor@Example.com ", "password": "demo-2026" });
    let res = app
        .call(Method::POST, "/api/auth/login", None, Some(&body))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let json = res.json();
    assert_eq!(json["principal"]["kind"], "customer");
    assert_eq!(json["principal"]["name"], "Amara Okafor");
    let token = json["token"].as_str().unwrap();

    let me = app.get("/api/auth/me", token).await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["principal"]["email"], "amara.okafor@example.com");

    let admin = app.login("ngozi.adeyemi@worknoon.example").await;
    assert_eq!(
        app.get("/api/auth/me", &admin).await.json()["principal"]["kind"],
        "admin"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_credentials_are_rejected_alike(pool: PgPool) {
    let app = TestApp::new(pool).await;
    for body in [
        json!({ "email": "amara.okafor@example.com", "password": "wrong" }),
        json!({ "email": "nobody@example.com", "password": "demo-2026" }),
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
    let token = app.login("sofia.rossi@example.com").await;
    let other = app.login("sofia.rossi@example.com").await;
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
    let token = app.login("tomas.herrera@example.com").await;
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
    assert_eq!(accounts[0]["name"], "Ngozi Adeyemi");
    assert_eq!(accounts[0]["title"], "Support admin");
    assert_eq!(accounts[0]["expected_verdict"], json!(null));
    assert_eq!(
        accounts[2],
        json!({
            "name": "Amara Okafor",
            "email": "amara.okafor@example.com",
            "role": "customer",
            "scenario": "clean_damaged",
            "title": "Damaged item",
            "description": db::seed::SCENARIOS[0].summary,
            "expected_verdict": "approved",
            "order_ref": "ORD-10437",
        })
    );
    let keys: Vec<_> = accounts[2..]
        .iter()
        .map(|a| a["scenario"].as_str().unwrap())
        .collect();
    let matrix: Vec<_> = db::seed::SCENARIOS.iter().map(|s| s.key).collect();
    assert_eq!(keys, matrix, "customers follow the scenario matrix");
    assert!(accounts.iter().all(|a| a.get("password_hash").is_none()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn repeated_failed_sign_ins_pause_the_email(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let attempt = |email: &'static str, password: &'static str| {
        let body = json!({ "email": email, "password": password });
        let app = &app;
        async move {
            app.call(Method::POST, "/api/auth/login", None, Some(&body))
                .await
        }
    };
    for left in [4, 3, 2, 1] {
        let res = attempt("grace.liu@example.com", "wrong").await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED);
        assert_eq!(res.json()["error"]["attempts_left"], left);
    }
    let res = attempt("GRACE.LIU@example.com", "wrong").await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.error_code(), "rate_limited");
    assert_eq!(res.headers["retry-after"], "300");

    // While paused, even the right password is refused; other emails are not.
    let res = attempt("grace.liu@example.com", "demo-2026").await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        attempt("amara.okafor@example.com", "demo-2026")
            .await
            .status,
        StatusCode::OK
    );

    // Unknown emails are counted the same way, so responses reveal nothing.
    for _ in 0..4 {
        assert_eq!(
            attempt("nobody@example.com", "x").await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        attempt("nobody@example.com", "x").await.status,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_successful_sign_in_resets_the_count(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let wrong = json!({ "email": "hana.sato@example.com", "password": "wrong" });
    for _ in 0..3 {
        app.call(Method::POST, "/api/auth/login", None, Some(&wrong))
            .await;
    }
    app.login("hana.sato@example.com").await;
    let res = app
        .call(Method::POST, "/api/auth/login", None, Some(&wrong))
        .await;
    assert_eq!(res.json()["error"]["attempts_left"], 4);
}
