//! Story 13.1 — consent + audit storage for the public benchmark dataset.
//!
//! Two operations: `record_optin` and `record_optout`. Both append a row
//! to `benchmark_consent`; the redactor reads the most-recent row to
//! decide whether the current operator is on the current
//! `TERMS_VERSION`.

use chrono::{DateTime, Utc};
use opengeo_core::ids::ProjectId;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct ConsentRow {
    pub id: Uuid,
    pub project_id: ProjectId,
    pub event: String,
    pub terms_version: String,
    pub actor: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct BenchmarkConsentRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> BenchmarkConsentRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn record_optin(
        &self,
        project_id: ProjectId,
        terms_version: &str,
        actor: Option<&str>,
        note: Option<&str>,
    ) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"INSERT INTO benchmark_consent
               (id, project_id, event, terms_version, actor, note)
               VALUES ($1, $2, 'optin', $3, $4, $5)"#,
        )
        .bind(id)
        .bind(project_id)
        .bind(terms_version)
        .bind(actor)
        .bind(note)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    pub async fn record_optout(
        &self,
        project_id: ProjectId,
        terms_version: &str,
        actor: Option<&str>,
        note: Option<&str>,
    ) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"INSERT INTO benchmark_consent
               (id, project_id, event, terms_version, actor, note)
               VALUES ($1, $2, 'optout', $3, $4, $5)"#,
        )
        .bind(id)
        .bind(project_id)
        .bind(terms_version)
        .bind(actor)
        .bind(note)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Most-recent consent row for this project, or `None` if the
    /// project has never opt'd in. Caller decides whether the redactor
    /// may emit payloads (active = most recent event is 'optin' AND
    /// terms_version matches `TERMS_VERSION`).
    pub async fn latest_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<ConsentRow>, Error> {
        let row = sqlx::query(
            r#"SELECT id, project_id, event, terms_version, actor, note, created_at
               FROM benchmark_consent
               WHERE project_id = $1
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(project_id)
        .fetch_optional(self.pool)
        .await?;
        row.map(|r| {
            Ok(ConsentRow {
                id: r.try_get("id")?,
                project_id: r.try_get("project_id")?,
                event: r.try_get("event")?,
                terms_version: r.try_get("terms_version")?,
                actor: r.try_get("actor")?,
                note: r.try_get("note")?,
                created_at: r.try_get("created_at")?,
            })
        })
        .transpose()
    }
}
