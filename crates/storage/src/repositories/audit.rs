//! Site-audit history persistence (Epic 32). Runtime queries (no compile-time
//! macros) so the offline sqlx cache needs no regen for this table.

use opengeo_core::ids::ProjectId;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Error;
use crate::models::AuditRunSummary;

pub struct AuditRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> AuditRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Persist one audit run. Returns the new row id.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_run(
        &self,
        id: Uuid,
        project_id: ProjectId,
        target: &str,
        overall_score: i16,
        pages_crawled: i32,
        gate_passed: Option<bool>,
        report: &JsonValue,
    ) -> Result<Uuid, Error> {
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        sqlx::query(
            r#"
            INSERT INTO audit_runs
                (id, project_id, target, overall_score, pages_crawled, gate_passed, report)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(pid)
        .bind(target)
        .bind(overall_score)
        .bind(pages_crawled)
        .bind(gate_passed)
        .bind(report)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Recent audit runs for a project, newest first.
    pub async fn list_runs_for_project(
        &self,
        project_id: ProjectId,
        limit: i64,
    ) -> Result<Vec<AuditRunSummary>, Error> {
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, AuditRunSummary>(
            r#"
            SELECT id, target, overall_score, pages_crawled, gate_passed, created_at
            FROM audit_runs
            WHERE project_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2
            "#,
        )
        .bind(pid)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Full stored report for one run (project-scoped), if present.
    pub async fn get_report(
        &self,
        id: Uuid,
        project_id: ProjectId,
    ) -> Result<Option<JsonValue>, Error> {
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let row: Option<(JsonValue,)> =
            sqlx::query_as("SELECT report FROM audit_runs WHERE id = $1 AND project_id = $2")
                .bind(id)
                .bind(pid)
                .fetch_optional(self.pool)
                .await?;
        Ok(row.map(|r| r.0))
    }
}
