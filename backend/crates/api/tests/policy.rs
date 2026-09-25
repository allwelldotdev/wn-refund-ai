mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};
use sqlx::PgPool;

const POLICY_MD: &str = include_str!("../../../../policy/refund-policy.md");

fn default_rules() -> Value {
    serde_json::from_str(api::DEFAULT_POLICY_JSON).unwrap()
}

fn rules_with_window(days: i64) -> Value {
    let mut rules = default_rules();
    rules["rules"][1]["days"] = days.into();
    rules
}

#[sqlx::test(migrations = "../../migrations")]
async fn customers_read_the_rendered_policy_but_not_admin_routes(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let token = app.login("alice@example.com").await;
    let res = app.get("/api/policy", &token).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json(), json!({ "version": 1, "prose": POLICY_MD }));

    for path in ["/api/admin/policy/current", "/api/admin/policy/versions"] {
        let res = app.get(path, &token).await;
        assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(res.error_code(), "admin_only");
    }
    let res = app
        .post(
            "/api/admin/policy/preview",
            &token,
            json!({ "rules": default_rules() }),
        )
        .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn admin_edits_then_reverts(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.login("admin@example.com").await;
    let current = app.get("/api/admin/policy/current", &admin).await.json();
    assert_eq!(current["version"]["version"], 1);
    assert_eq!(current["version"]["author_kind"], "system");
    assert_eq!(current["rules"], default_rules());
    assert_eq!(current["prose"], POLICY_MD);
    let v1_id = current["version"]["id"].as_str().unwrap().to_owned();

    let res = app
        .post(
            "/api/admin/policy/versions",
            &admin,
            json!({ "base_version_id": v1_id, "rules": rules_with_window(14), "change_note": " Holiday rules " }),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let v2 = res.json();
    assert_eq!(v2["version"]["version"], 2);
    assert_eq!(v2["version"]["author_name"], "Sam Admin");
    assert_eq!(v2["version"]["change_note"], "Holiday rules");
    assert!(v2["prose"].as_str().unwrap().contains("within 14 days"));

    let customer = app.login("alice@example.com").await;
    assert_eq!(app.get("/api/policy", &customer).await.json()["version"], 2);

    let res = app
        .post(
            &format!("/api/admin/policy/versions/{v1_id}/revert"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let v3 = res.json();
    assert_eq!(v3["version"]["version"], 3);
    assert_eq!(v3["version"]["reverted_from_version"], 1);
    assert_eq!(v3["version"]["change_note"], "Reverted to version 1");
    assert_eq!(v3["prose"], POLICY_MD);

    let versions = app.get("/api/admin/policy/versions", &admin).await.json();
    let numbers: Vec<i64> = versions
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["version"].as_i64().unwrap())
        .collect();
    assert_eq!(numbers, [3, 2, 1]);

    let res = app
        .get(&format!("/api/admin/policy/versions/{v1_id}"), &admin)
        .await;
    assert_eq!(res.json()["rules"], default_rules());
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_no_op_invalid_and_missing_versions(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.login("admin@example.com").await;
    let v1_id = app.get("/api/admin/policy/current", &admin).await.json()["version"]["id"].clone();
    let create = |rules: Value, base: &Value| json!({ "base_version_id": base, "rules": rules });

    let res = app
        .post(
            "/api/admin/policy/versions",
            &admin,
            create(default_rules(), &v1_id),
        )
        .await;
    assert_eq!(
        (res.status, res.error_code().as_str()),
        (StatusCode::CONFLICT, "no_op")
    );

    let res = app
        .post(
            "/api/admin/policy/versions",
            &admin,
            create(rules_with_window(14), &v1_id),
        )
        .await;
    assert_eq!(res.status, StatusCode::CREATED);

    let res = app
        .post(
            "/api/admin/policy/versions",
            &admin,
            create(rules_with_window(10), &v1_id),
        )
        .await;
    assert_eq!(res.status, StatusCode::CONFLICT);
    let body = res.json();
    assert_eq!(body["error"]["code"], "stale_base");
    assert_eq!(body["error"]["latest"]["version"]["version"], 2);
    assert_eq!(body["error"]["latest"]["rules"], rules_with_window(14));

    let res = app
        .post(
            "/api/admin/policy/versions",
            &admin,
            create(rules_with_window(0), &v1_id),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res.json()["error"]["fields"],
        json!([{ "path": "rules[1].days", "message": "must be between 1 and 365" }])
    );

    let mut unknown = default_rules();
    unknown["rules"][0]["kind"] = "vip_always_approved".into();
    let res = app
        .post(
            "/api/admin/policy/preview",
            &admin,
            json!({ "rules": unknown }),
        )
        .await;
    assert_eq!(res.status, StatusCode::UNPROCESSABLE_ENTITY);
    let path = res.json()["error"]["fields"][0]["path"].clone();
    assert!(path.as_str().unwrap().starts_with("rules[0]"), "{path}");

    let missing = "00000000-0000-0000-0000-000000000000";
    let res = app
        .post(
            &format!("/api/admin/policy/versions/{missing}/revert"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let res = app
        .get(&format!("/api/admin/policy/versions/{missing}"), &admin)
        .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn preview_renders_without_saving(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.login("ops@example.com").await;
    let res = app
        .post(
            "/api/admin/policy/preview",
            &admin,
            json!({ "rules": rules_with_window(7) }),
        )
        .await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(
        res.json()["prose"]
            .as_str()
            .unwrap()
            .contains("within 7 days")
    );
    let versions = app.get("/api/admin/policy/versions", &admin).await.json();
    assert_eq!(versions.as_array().unwrap().len(), 1);
}
