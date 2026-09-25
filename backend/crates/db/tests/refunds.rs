//! Conversation lock and refund-request invariants against a real Postgres.

use std::time::Duration;

use db::lock::lock_conversation;
use db::refunds::{NewAudit, NewRefundRequest, create_decided};
use db::{Db, DbError, seed};
use domain::types::{ReasonCategory, RequestState, Verdict};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const DEFAULT_POLICY: &str = include_str!("../../../../policy/default-policy.json");

#[sqlx::test(migrations = "../../migrations")]
async fn the_lock_serialises_runs_and_drop_frees_it(pool: PgPool) {
    let db = Db(pool);
    let id = Uuid::new_v4();
    let first = lock_conversation(&db, id).await.unwrap();

    let other = db.clone();
    let waiter = tokio::spawn(async move {
        lock_conversation(&other, id)
            .await
            .unwrap()
            .release()
            .await
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!waiter.is_finished(), "second run must wait for the first");
    first.release().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), waiter)
        .await
        .expect("released lock is acquired")
        .unwrap();

    // A lock that is dropped without release (panic, early return) must not
    // stay held on a pooled connection.
    drop(lock_conversation(&db, id).await.unwrap());
    let again = tokio::time::timeout(Duration::from_secs(5), lock_conversation(&db, id))
        .await
        .expect("dropped lock is freed")
        .unwrap();
    again.release().await.unwrap();
}

async fn conversation(db: &Db, customer_id: Uuid) -> Uuid {
    db::conversations::create_conversation(db, customer_id)
        .await
        .unwrap()
        .id
}

fn audit(policy_version_id: Uuid, verdict: Verdict) -> NewAudit {
    NewAudit {
        policy_version_id,
        content_hash: "test".into(),
        evaluated_through_seq: 1,
        verdict,
        prescan_signals: json!([]),
        extracted: None,
        facts: json!({}),
        rule_trace: json!([]),
        flags: json!([]),
        stages: json!({}),
        fired_kinds: vec![],
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_item_is_approved_at_most_once(pool: PgPool) {
    let db = Db(pool.clone());
    seed::run(&db, DEFAULT_POLICY).await.unwrap();
    let policy = db::policy::latest_policy(&db).await.unwrap().meta.id;
    let alice = seed::stable_id("customer", "alice@example.com");
    let request = |conversation_id, state| NewRefundRequest {
        conversation_id,
        customer_id: alice,
        order_id: Some(seed::stable_id("order", "ORD-1001")),
        order_item_id: Some(seed::item_id("ORD-1001", 0)),
        amount_cents: Some(8999),
        reason_category: Some(ReasonCategory::Damaged),
        state,
    };
    let mut conn = pool.acquire().await.unwrap();

    let a = conversation(&db, alice).await;
    let created = create_decided(
        &mut conn,
        &request(a, RequestState::Approved),
        &audit(policy, Verdict::Approved),
    )
    .await
    .unwrap();
    assert_eq!(created.request_ref, "RR-1001");

    let b = conversation(&db, alice).await;
    let dup = create_decided(
        &mut conn,
        &request(b, RequestState::Approved),
        &audit(policy, Verdict::Approved),
    )
    .await;
    assert!(matches!(
        dup,
        Err(DbError::Conflict("duplicate_active_refund"))
    ));

    let escalated = create_decided(
        &mut conn,
        &request(b, RequestState::Escalated),
        &audit(policy, Verdict::Escalated),
    )
    .await
    .unwrap();
    let pending: String =
        sqlx::query_scalar("SELECT status FROM escalation_reviews WHERE refund_request_id = $1")
            .bind(escalated.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, "pending");

    let second = create_decided(
        &mut conn,
        &request(a, RequestState::Denied),
        &audit(policy, Verdict::Denied),
    )
    .await;
    assert!(matches!(second, Err(DbError::Conflict("request_exists"))));
}
