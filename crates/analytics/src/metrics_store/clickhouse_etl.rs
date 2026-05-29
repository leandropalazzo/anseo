//! Phase 2 Story 14.1 — Postgres → ClickHouse ETL (basic, non-resumable).
//!
//! Drains the existing per-project Postgres aggregations
//! (`visibility_trend`, `citation_summary`) into the pre-aggregated
//! ClickHouse tables defined by [`super::clickhouse::SCHEMA_DDL`]. The
//! idempotent path uses ClickHouse's `INSERT … VALUES` semantics: each
//! invocation truncates the target rows for the project first, then
//! re-inserts. Resumable / batched migration with the
//! `analytics_migration_state` checkpoint table is the natural Story
//! 14.1 follow-up; this minimal ETL exists so a fresh ClickHouse
//! deployment can be primed in one shot.

use chrono::{Duration, Utc};
use opengeo_core::ProjectId;
use opengeo_storage::Storage;

use crate::{citation_summary, visibility_trend};
use crate::metrics_store::clickhouse::{ClickHouseError, ClickHouseMetricsStore};

#[derive(Debug, thiserror::Error)]
pub enum EtlError {
    #[error("Postgres read failed: {0}")]
    Postgres(#[from] opengeo_storage::Error),
    #[error("ClickHouse write failed: {0}")]
    ClickHouse(#[from] ClickHouseError),
}

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub project_id: Option<ProjectId>,
    pub visibility_rows_migrated: usize,
    pub citation_rows_migrated: usize,
}

/// Migrate one project's analytics view into ClickHouse. Idempotent:
/// the prior project rows are deleted before the fresh batch is
/// inserted.
pub async fn migrate_project(
    storage: &Storage,
    ch: &ClickHouseMetricsStore,
    project_id: ProjectId,
    prompt_slugs: &[&str],
    days: i32,
    citation_limit: i64,
) -> Result<MigrationReport, EtlError> {
    ch.ensure_schema().await?;
    let mut report = MigrationReport {
        project_id: Some(project_id),
        ..Default::default()
    };

    // Idempotent reset: wipe this project's rows before the re-insert.
    let pid_uuid = pid_uuid(project_id);
    ch.execute(&format!(
        "ALTER TABLE visibility_points DELETE WHERE project_id = '{pid_uuid}'"
    ))
    .await?;
    ch.execute(&format!(
        "ALTER TABLE citation_totals DELETE WHERE project_id = '{pid_uuid}'"
    ))
    .await?;

    // Visibility: one Postgres trend query per declared prompt.
    let cutoff = Utc::now() - Duration::days(days as i64);
    let _ = cutoff; // kept for future per-prompt sampling
    for slug in prompt_slugs {
        let points = visibility_trend(storage, project_id, slug, days).await?;
        if points.is_empty() {
            continue;
        }
        let typed: Vec<(ProjectId, &str, &str, _, _, _)> = points
            .iter()
            .map(|p| {
                (
                    project_id,
                    *slug,
                    p.provider.as_str(),
                    p.bucket_start,
                    p.avg_rank,
                    p.presence_rate,
                )
            })
            .collect();
        ch.seed_visibility_points(&typed).await?;
        report.visibility_rows_migrated += points.len();
    }

    // Citations: one Postgres top-N query collapses to the
    // citation_totals shape.
    let summary = citation_summary(storage, project_id, citation_limit).await?;
    if !summary.is_empty() {
        let typed: Vec<(ProjectId, &str, i64, Option<&str>)> = summary
            .iter()
            .map(|r| {
                (
                    project_id,
                    r.domain.as_str(),
                    r.frequency,
                    r.source_type.as_deref(),
                )
            })
            .collect();
        ch.seed_citation_totals(&typed).await?;
        report.citation_rows_migrated = summary.len();
    }

    Ok(report)
}

fn pid_uuid(project_id: ProjectId) -> String {
    let bytes: [u8; 16] = project_id.into_ulid().to_bytes();
    uuid::Uuid::from_bytes(bytes).to_string()
}
