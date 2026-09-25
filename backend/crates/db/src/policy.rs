//! Append-only policy versions (ADR-008, ADR-016). An edit names the version it
//! was based on; if another admin saved first, the edit is refused with the
//! latest version instead of silently overwriting it.

use chrono::{DateTime, Utc};
use domain::policy::Policy;
use serde::Serialize;
use uuid::Uuid;

use crate::{Db, DbError, from_json};

/// Serialises policy writers so each sees the version the previous one committed.
const POLICY_LOCK_KEY: i64 = 0x0090_11C7;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyVersionMeta {
    pub id: Uuid,
    pub version: i32,
    pub content_hash: String,
    pub author_kind: String,
    pub author_name: Option<String>,
    pub change_note: Option<String>,
    pub reverted_from_version_id: Option<Uuid>,
    pub reverted_from_version: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyVersionRow {
    pub meta: PolicyVersionMeta,
    pub rules: Policy,
}

/// One query shape for every read, so the metadata is built the same way.
macro_rules! select_versions {
    ($where:literal $(, $arg:expr)*) => {
        sqlx::query!(
            r#"SELECT p.id, p.version, p.rules, p.content_hash, p.author_kind,
                      a.name AS "author_name?", p.change_note, p.reverted_from_version_id,
                      r.version AS "reverted_from_version?", p.created_at
               FROM policy_versions p
               LEFT JOIN admins a ON a.id = p.author_admin_id
               LEFT JOIN policy_versions r ON r.id = p.reverted_from_version_id "#
                + $where,
            $($arg),*
        )
    };
}

macro_rules! into_row {
    ($r:expr) => {{
        let r = $r;
        Ok::<_, DbError>(PolicyVersionRow {
            rules: from_json(r.rules)?,
            meta: PolicyVersionMeta {
                id: r.id,
                version: r.version,
                content_hash: r.content_hash,
                author_kind: r.author_kind,
                author_name: r.author_name,
                change_note: r.change_note,
                reverted_from_version_id: r.reverted_from_version_id,
                reverted_from_version: r.reverted_from_version,
                created_at: r.created_at,
            },
        })
    }};
}

pub async fn latest_policy(db: &Db) -> Result<PolicyVersionRow, DbError> {
    let r = select_versions!("ORDER BY p.version DESC LIMIT 1")
        .fetch_optional(&db.0)
        .await?
        .ok_or(DbError::NotFound)?;
    into_row!(r)
}

pub async fn get_policy_version(db: &Db, id: Uuid) -> Result<Option<PolicyVersionRow>, DbError> {
    let r = select_versions!("WHERE p.id = $1", id)
        .fetch_optional(&db.0)
        .await?;
    r.map(|r| into_row!(r)).transpose()
}

/// Newest first.
pub async fn list_policy_versions(db: &Db) -> Result<Vec<PolicyVersionMeta>, DbError> {
    let rows = select_versions!("ORDER BY p.version DESC")
        .fetch_all(&db.0)
        .await?;
    rows.into_iter()
        .map(|r| into_row!(r).map(|v| v.meta))
        .collect()
}

pub struct NewPolicyVersion<'a> {
    pub rules: &'a Policy,
    /// The version the admin edited. `None` (reverts) skips the stale check.
    pub base_version_id: Option<Uuid>,
    pub author_admin_id: Uuid,
    pub change_note: Option<&'a str>,
    pub reverted_from: Option<Uuid>,
}

/// Inserts `latest + 1`. `StaleBase` if `base_version_id` is not the latest;
/// `NoOp` if the rules hash equals the latest version's.
pub async fn insert_policy_version(
    db: &Db,
    new: NewPolicyVersion<'_>,
) -> Result<PolicyVersionRow, DbError> {
    let hash = new.rules.content_hash();
    let rules = serde_json::to_value(new.rules).expect("policy serializes");
    let mut tx = db.0.begin().await?;
    // An advisory lock rather than `SELECT … FOR UPDATE` on the latest row: after
    // waiting, FOR UPDATE would still return the row it first found, missing a
    // version committed meanwhile. The next statement takes a fresh snapshot.
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", POLICY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    let latest = sqlx::query!(
        "SELECT id, version, content_hash FROM policy_versions ORDER BY version DESC LIMIT 1"
    )
    .fetch_one(&mut *tx)
    .await?;
    if new.base_version_id.is_some_and(|base| base != latest.id) {
        return Err(DbError::StaleBase {
            latest_id: latest.id,
            latest_version: latest.version,
        });
    }
    if latest.content_hash == hash {
        return Err(DbError::NoOp);
    }
    let id = sqlx::query_scalar!(
        "INSERT INTO policy_versions
           (version, rules, content_hash, author_kind, author_admin_id, change_note, reverted_from_version_id)
         VALUES ($1, $2, $3, 'admin', $4, $5, $6)
         RETURNING id",
        latest.version + 1,
        rules,
        hash,
        new.author_admin_id,
        new.change_note,
        new.reverted_from,
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    get_policy_version(db, id).await?.ok_or(DbError::NotFound)
}
