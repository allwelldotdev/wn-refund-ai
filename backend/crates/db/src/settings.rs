//! Admin settings that sit outside the versioned policy: a single row.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{Db, DbError};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Settings {
    /// Customers may dispute an automatic denial, once, from Your requests.
    pub allow_disputes: bool,
    /// Who last changed a setting, and when; `None` while still the default.
    pub updated_by: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub async fn get_settings(db: &Db) -> Result<Settings, DbError> {
    let r = sqlx::query!(
        r#"SELECT s.allow_disputes, a.name AS "updated_by?", s.updated_at
           FROM app_settings s
           LEFT JOIN admins a ON a.id = s.updated_by_admin_id"#
    )
    .fetch_one(&db.0)
    .await?;
    Ok(Settings {
        allow_disputes: r.allow_disputes,
        updated_by: r.updated_by,
        updated_at: r.updated_at,
    })
}

/// Records who changed it only when the value actually changes.
pub async fn set_allow_disputes(db: &Db, admin_id: Uuid, allow: bool) -> Result<Settings, DbError> {
    sqlx::query!(
        "UPDATE app_settings
         SET allow_disputes = $1, updated_by_admin_id = $2, updated_at = now()
         WHERE allow_disputes IS DISTINCT FROM $1",
        allow,
        admin_id,
    )
    .execute(&db.0)
    .await?;
    get_settings(db).await
}
