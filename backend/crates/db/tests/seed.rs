//! Seed behaviour against a real Postgres. `#[sqlx::test]` creates a fresh
//! database per test (DATABASE_URL must point at the compose postgres).

use db::{Db, seed};
use sqlx::PgPool;

const DEFAULT_POLICY: &str = include_str!("../../../../policy/default-policy.json");

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn seed_is_idempotent(pool: PgPool) {
    let db = Db(pool.clone());
    let first = seed::run(&db, DEFAULT_POLICY).await.unwrap();
    assert!(first.policy_seeded);
    assert_eq!(first.customers, 15);

    let counts = |p: PgPool| async move {
        let mut v = Vec::new();
        for t in [
            "customers",
            "admins",
            "orders",
            "order_items",
            "conversations",
            "messages",
            "refund_requests",
            "request_events",
            "policy_versions",
        ] {
            v.push((t, count(&p, t).await));
        }
        v
    };
    let before = counts(pool.clone()).await;

    let second = seed::run(&db, DEFAULT_POLICY).await.unwrap();
    assert!(!second.policy_seeded, "policy v1 must only be seeded once");
    assert_eq!(before, counts(pool.clone()).await);

    assert_eq!(count(&pool, "customers").await, 15);
    assert_eq!(count(&pool, "admins").await, 2);
    assert_eq!(count(&pool, "orders").await, 18);
    assert_eq!(count(&pool, "order_items").await, 19);
    assert_eq!(count(&pool, "refund_requests").await, 3);
    assert_eq!(count(&pool, "policy_versions").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn policy_v1_is_system_authored_with_matching_hash(pool: PgPool) {
    seed::run(&Db(pool.clone()), DEFAULT_POLICY).await.unwrap();
    let (version, author, hash, rules): (i32, String, String, serde_json::Value) =
        sqlx::query_as("SELECT version, author_kind, content_hash, rules FROM policy_versions")
            .fetch_one(&pool)
            .await
            .unwrap();
    let expected = domain::policy::Policy::parse(DEFAULT_POLICY).unwrap();
    assert_eq!((version, author.as_str()), (1, "system"));
    assert_eq!(hash, expected.content_hash());
    let stored: domain::policy::Policy = serde_json::from_value(rules).unwrap();
    assert_eq!(stored, expected);
}

#[sqlx::test(migrations = "../../migrations")]
async fn existing_policy_is_never_overwritten(pool: PgPool) {
    let db = Db(pool.clone());
    seed::run(&db, DEFAULT_POLICY).await.unwrap();
    sqlx::query("UPDATE policy_versions SET change_note = 'marker'")
        .execute(&pool)
        .await
        .unwrap();
    let other = r#"{"rules":[{"kind":"final_sale_not_refundable","enabled":true}]}"#;
    let report = seed::run(&db, other).await.unwrap();
    assert!(!report.policy_seeded);
    let note: Option<String> = sqlx::query_scalar("SELECT change_note FROM policy_versions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(note.as_deref(), Some("marker"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn reseed_refreshes_relative_dates(pool: PgPool) {
    let db = Db(pool.clone());
    seed::run(&db, DEFAULT_POLICY).await.unwrap();
    // Simulate a database created 30 days ago.
    sqlx::query("UPDATE orders SET placed_at = placed_at - interval '30 days', delivered_at = delivered_at - interval '30 days'")
        .execute(&pool)
        .await
        .unwrap();
    seed::run(&db, DEFAULT_POLICY).await.unwrap();
    let age_days: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM now() - delivered_at)::float8 / 86400 FROM orders WHERE ref = 'ORD-1001'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (age_days - 9.0).abs() < 0.01,
        "ORD-1001 should be delivered 9 days ago, got {age_days}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn demo_password_verifies(pool: PgPool) {
    use argon2::password_hash::{PasswordVerifier, phc::PasswordHash};
    seed::run(&Db(pool.clone()), DEFAULT_POLICY).await.unwrap();
    let hash: String =
        sqlx::query_scalar("SELECT password_hash FROM customers WHERE email = 'alice@example.com'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let parsed = PasswordHash::new(&hash).unwrap();
    assert!(
        argon2::Argon2::default()
            .verify_password(seed::DEMO_PASSWORD.as_bytes(), &parsed)
            .is_ok()
    );
}
