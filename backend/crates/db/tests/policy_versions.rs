//! Policy versioning against a real Postgres.

use db::policy::{self, NewPolicyVersion};
use db::{Db, DbError, seed};
use domain::policy::{Policy, Rule};
use sqlx::PgPool;
use uuid::Uuid;

const DEFAULT_POLICY: &str = include_str!("../../../../policy/default-policy.json");

async fn seeded(pool: PgPool) -> Db {
    let db = Db(pool);
    seed::run(&db, DEFAULT_POLICY).await.unwrap();
    db
}

fn admin() -> Uuid {
    seed::stable_id("admin", "ngozi.adeyemi@worknoon.example")
}

fn with_window(days: i64) -> Policy {
    let mut p = Policy::parse(DEFAULT_POLICY).unwrap();
    for rule in &mut p.rules {
        if let Rule::RefundWindow { days: d, .. } = rule {
            *d = days;
        }
    }
    p
}

fn edit(rules: &Policy, base: Uuid) -> NewPolicyVersion<'_> {
    NewPolicyVersion {
        rules,
        base_version_id: Some(base),
        author_admin_id: admin(),
        change_note: Some("shorter window"),
        reverted_from: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn edit_on_the_latest_version_creates_the_next_one(pool: PgPool) {
    let db = seeded(pool).await;
    let v1 = policy::latest_policy(&db).await.unwrap();
    let rules = with_window(5);
    let v2 = policy::insert_policy_version(&db, edit(&rules, v1.meta.id))
        .await
        .unwrap();
    assert_eq!(v2.meta.version, 2);
    assert_eq!(v2.rules, rules);
    assert_eq!(v2.meta.content_hash, rules.content_hash());
    assert_eq!(v2.meta.author_kind, "admin");
    assert_eq!(v2.meta.author_name.as_deref(), Some("Ngozi Adeyemi"));
    assert_eq!(v2.meta.change_note.as_deref(), Some("shorter window"));

    assert_eq!(policy::latest_policy(&db).await.unwrap(), v2);
    let listed: Vec<i32> = policy::list_policy_versions(&db)
        .await
        .unwrap()
        .iter()
        .map(|m| m.version)
        .collect();
    assert_eq!(listed, [2, 1]);
    assert_eq!(v1.meta.author_name, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_base_and_unchanged_rules_are_refused(pool: PgPool) {
    let db = seeded(pool).await;
    let v1 = policy::latest_policy(&db).await.unwrap();
    let v2 = policy::insert_policy_version(&db, edit(&with_window(5), v1.meta.id))
        .await
        .unwrap();

    let stale = policy::insert_policy_version(&db, edit(&with_window(3), v1.meta.id)).await;
    assert!(matches!(
        stale,
        Err(DbError::StaleBase { latest_id, latest_version: 2 }) if latest_id == v2.meta.id
    ));

    let same = policy::insert_policy_version(&db, edit(&with_window(5), v2.meta.id)).await;
    assert!(matches!(same, Err(DbError::NoOp)));
    assert_eq!(policy::list_policy_versions(&db).await.unwrap().len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_edits_on_the_same_base_let_exactly_one_win(pool: PgPool) {
    let db = seeded(pool).await;
    let base = policy::latest_policy(&db).await.unwrap().meta.id;
    let (a, b) = (with_window(5), with_window(3));
    let (ra, rb) = tokio::join!(
        policy::insert_policy_version(&db, edit(&a, base)),
        policy::insert_policy_version(&db, edit(&b, base)),
    );
    let results = [ra, rb];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results.iter().any(|r| matches!(
        r,
        Err(DbError::StaleBase {
            latest_version: 2,
            ..
        })
    )));
}

#[sqlx::test(migrations = "../../migrations")]
async fn revert_copies_old_rules_and_records_the_source(pool: PgPool) {
    let db = seeded(pool).await;
    let v1 = policy::latest_policy(&db).await.unwrap();
    policy::insert_policy_version(&db, edit(&with_window(5), v1.meta.id))
        .await
        .unwrap();
    let v3 = policy::insert_policy_version(
        &db,
        NewPolicyVersion {
            rules: &v1.rules,
            base_version_id: None,
            author_admin_id: admin(),
            change_note: None,
            reverted_from: Some(v1.meta.id),
        },
    )
    .await
    .unwrap();
    assert_eq!(v3.meta.version, 3);
    assert_eq!(v3.rules, v1.rules);
    assert_eq!(v3.meta.content_hash, v1.meta.content_hash);
    assert_eq!(v3.meta.reverted_from_version_id, Some(v1.meta.id));
    assert_eq!(v3.meta.reverted_from_version, Some(1));
}
