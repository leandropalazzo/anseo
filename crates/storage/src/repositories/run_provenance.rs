//! Run provenance repository (Story 31-3).
//!
//! Append-only lifecycle log for a prompt run: one row per stage
//! (`provider_call`, `response_persisted`, `mention_extraction`,
//! `citation_extraction`, `ranking`). Written by the orchestrator write path
//! (`crates/providers/src/persistence.rs`) and read by
//! `GET /runs/:id/provenance`.
//!
//! Uses RUNTIME `sqlx::query` / `query_as` (no `query!` macros) so the offline
//! `.sqlx/` cache stays untouched.

use anseo_core::ids::PromptRunId;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Error;

/// One persisted provenance step.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RunProvenanceRow {
    pub id: Uuid,
    pub prompt_run_id: PromptRunId,
    pub step: String,
    pub status: String,
    pub detail: serde_json::Value,
    pub at: DateTime<Utc>,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
}

/// Lifecycle status of a provenance step. Maps onto the `status` CHECK
/// constraint (`ok` | `error` | `skipped`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Ok,
    Error,
    Skipped,
}

impl StepStatus {
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Error => "error",
            Self::Skipped => "skipped",
        }
    }
}

pub struct RunProvenanceRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RunProvenanceRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Append one provenance step for a run. `detail` is free-form JSON
    /// (defaults to `{}` when callers have nothing to attach). The `at`
    /// timestamp is assigned by the database default so ordering reflects
    /// true write order.
    pub async fn record(
        &self,
        prompt_run_id: PromptRunId,
        step: &str,
        status: StepStatus,
        detail: serde_json::Value,
    ) -> Result<(), Error> {
        let rid = uuid::Uuid::from_bytes(prompt_run_id.into_ulid().to_bytes());
        sqlx::query(
            r#"
            INSERT INTO run_provenance (prompt_run_id, step, status, detail)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(rid)
        .bind(step)
        .bind(status.as_wire_str())
        .bind(detail)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// All provenance steps for a run, oldest first (by `at`, then `id` for a
    /// stable tiebreak when steps share a timestamp).
    pub async fn list_by_run(
        &self,
        prompt_run_id: PromptRunId,
    ) -> Result<Vec<RunProvenanceRow>, Error> {
        let rid = uuid::Uuid::from_bytes(prompt_run_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, RunProvenanceRow>(
            r#"
            SELECT id, prompt_run_id, step, status, detail, at,
                   organization_id, tenant_id
            FROM run_provenance
            WHERE prompt_run_id = $1
            ORDER BY at ASC, id ASC
            "#,
        )
        .bind(rid)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}
