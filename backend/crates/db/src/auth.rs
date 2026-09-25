//! Accounts and bearer sessions. Password verification happens in `api`; this
//! module only stores and looks up.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Customer,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Customer => "customer",
            Role::Admin => "admin",
        }
    }

    fn parse(s: &str) -> Result<Role, DbError> {
        match s {
            "customer" => Ok(Role::Customer),
            "admin" => Ok(Role::Admin),
            other => Err(DbError::Corrupt(format!(
                "unknown principal kind `{other}`"
            ))),
        }
    }
}

/// Who a session belongs to. Serialized as `{kind, id, name, email}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Principal {
    pub kind: Role,
    pub id: Uuid,
    pub name: String,
    pub email: String,
}

pub struct Account {
    pub principal: Principal,
    pub password_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DemoAccount {
    pub name: String,
    pub email: String,
    pub role: Role,
    pub scenario: Option<String>,
}

/// Customer and admin emails live in separate tables; the seed keeps them disjoint.
pub async fn find_account(db: &Db, email: &str) -> Result<Option<Account>, DbError> {
    let row = sqlx::query!(
        r#"SELECT 'customer' AS "kind!", id AS "id!", name AS "name!", email AS "email!",
                  password_hash AS "password_hash!"
           FROM customers WHERE email = $1
           UNION ALL
           SELECT 'admin', id, name, email, password_hash FROM admins WHERE email = $1
           LIMIT 1"#,
        email,
    )
    .fetch_optional(&db.0)
    .await?;
    row.map(|r| {
        Ok(Account {
            principal: Principal {
                kind: Role::parse(&r.kind)?,
                id: r.id,
                name: r.name,
                email: r.email,
            },
            password_hash: r.password_hash,
        })
    })
    .transpose()
}

/// Also deletes expired sessions, so the table does not grow without bound.
pub async fn create_session(
    db: &Db,
    principal: &Principal,
    expires_at: DateTime<Utc>,
) -> Result<Uuid, DbError> {
    let (customer_id, admin_id) = match principal.kind {
        Role::Customer => (Some(principal.id), None),
        Role::Admin => (None, Some(principal.id)),
    };
    sqlx::query!("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(&db.0)
        .await?;
    let id = sqlx::query_scalar!(
        "INSERT INTO sessions (principal_kind, customer_id, admin_id, expires_at)
         VALUES ($1, $2, $3, $4) RETURNING id",
        principal.kind.as_str(),
        customer_id,
        admin_id,
        expires_at,
    )
    .fetch_one(&db.0)
    .await?;
    Ok(id)
}

pub async fn find_valid_session(db: &Db, id: Uuid) -> Result<Option<Principal>, DbError> {
    let row = sqlx::query!(
        r#"SELECT s.principal_kind,
                  COALESCE(c.id, a.id) AS "id!",
                  COALESCE(c.name, a.name) AS "name!",
                  COALESCE(c.email, a.email) AS "email!"
           FROM sessions s
           LEFT JOIN customers c ON c.id = s.customer_id
           LEFT JOIN admins a ON a.id = s.admin_id
           WHERE s.id = $1 AND s.expires_at > now()"#,
        id,
    )
    .fetch_optional(&db.0)
    .await?;
    row.map(|r| {
        Ok(Principal {
            kind: Role::parse(&r.principal_kind)?,
            id: r.id,
            name: r.name,
            email: r.email,
        })
    })
    .transpose()
}

pub async fn delete_session(db: &Db, id: Uuid) -> Result<bool, DbError> {
    let deleted = sqlx::query!("DELETE FROM sessions WHERE id = $1", id)
        .execute(&db.0)
        .await?
        .rows_affected();
    Ok(deleted == 1)
}

/// Every seeded account, admins first, for the login page.
pub async fn list_demo_accounts(db: &Db) -> Result<Vec<DemoAccount>, DbError> {
    let rows = sqlx::query!(
        r#"SELECT name AS "name!", email AS "email!", role AS "role!", scenario
           FROM (SELECT name, email, 'admin' AS role, NULL::text AS scenario FROM admins
                 UNION ALL
                 SELECT name, email, 'customer', scenario FROM customers) accounts
           ORDER BY role, name"#
    )
    .fetch_all(&db.0)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(DemoAccount {
                name: r.name,
                email: r.email,
                role: Role::parse(&r.role)?,
                scenario: r.scenario,
            })
        })
        .collect()
}
