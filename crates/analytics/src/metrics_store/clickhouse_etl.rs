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

use crate::metrics_store::clickhouse::{
    ClickHouseError, ClickHouseMetricsStore, VisibilityPointRow,
};
use crate::{citation_summary, visibility_trend};

#[derive(Debug, thiserror::Error)]
pub enum EtlError {
    #[error("Postgres read failed: {0}")]
    Postgres(#[from] opengeo_storage::Error),
    #[error("ClickHouse write failed: {0}")]
    ClickHouse(#[from] ClickHouseError),
    #[error("checkpoint query failed: {0}")]
    Sqlx(#[from] sqlx::Error),
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

// ---------------------------------------------------------------------------
// Story 0.1 — resumable, checkpointed ETL (architecture-phase3 §3.3 / D-8).
// ---------------------------------------------------------------------------

/// Default batch granularity for the resumable ETL (architecture AC-0.2:
/// `last_completed_batch_id` advances per 10 000-row batch).
pub const DEFAULT_ETL_BATCH_SIZE: usize = 10_000;

/// Knobs for [`migrate_project_resumable`].
#[derive(Debug, Clone)]
pub struct ResumableConfig {
    /// Rows per batch; the checkpoint advances once per fully-inserted batch.
    pub batch_size: usize,
    /// Test/operational hook: stop cleanly after committing this many *new*
    /// batches in the current invocation, leaving the run resumable
    /// (`finished_at` stays NULL). Models a SIGTERM mid-migration.
    pub stop_after_batches: Option<usize>,
}

impl Default for ResumableConfig {
    fn default() -> Self {
        Self {
            batch_size: DEFAULT_ETL_BATCH_SIZE,
            stop_after_batches: None,
        }
    }
}

/// Outcome of a resumable ETL invocation.
#[derive(Debug, Default)]
pub struct ResumableReport {
    pub project_id: Option<ProjectId>,
    pub total_batches: usize,
    /// Batches committed across all invocations (i.e. the checkpoint value).
    pub batches_completed: usize,
    /// Batches committed in *this* invocation.
    pub batches_this_run: usize,
    pub finished: bool,
}

#[derive(Clone)]
enum EtlRow {
    Visibility {
        slug: String,
        provider: String,
        bucket_start: chrono::DateTime<Utc>,
        avg_rank: Option<f64>,
        presence_rate: f64,
    },
    Citation {
        domain: String,
        frequency: i64,
        source_type: Option<String>,
    },
}

/// Resumable Postgres→ClickHouse ETL for one project.
///
/// The full row set (visibility points across declared prompts, then citation
/// totals) is materialized in a deterministic order and split into
/// `batch_size` batches. A checkpoint row in `analytics_migration_state`
/// records `last_completed_batch_id`; an interrupted run resumes from there
/// instead of re-inserting earlier batches. A fresh run (no checkpoint, or a
/// previously-finished one) wipes the project's ClickHouse rows first, so
/// interrupt+resume converges to the same state as a single pass.
pub async fn migrate_project_resumable(
    storage: &Storage,
    ch: &ClickHouseMetricsStore,
    project_id: ProjectId,
    prompt_slugs: &[&str],
    days: i32,
    citation_limit: i64,
    cfg: &ResumableConfig,
) -> Result<ResumableReport, EtlError> {
    ch.ensure_schema().await?;
    let pool = storage.pool();
    let batch_size = cfg.batch_size.max(1);

    // Materialize the full, deterministically-ordered row set.
    let mut rows: Vec<EtlRow> = Vec::new();
    for slug in prompt_slugs {
        let points = visibility_trend(storage, project_id, slug, days).await?;
        for p in points {
            rows.push(EtlRow::Visibility {
                slug: (*slug).to_string(),
                provider: p.provider,
                bucket_start: p.bucket_start,
                avg_rank: p.avg_rank,
                presence_rate: p.presence_rate,
            });
        }
    }
    for r in citation_summary(storage, project_id, citation_limit).await? {
        rows.push(EtlRow::Citation {
            domain: r.domain,
            frequency: r.frequency,
            source_type: r.source_type,
        });
    }

    let batches: Vec<&[EtlRow]> = if rows.is_empty() {
        Vec::new()
    } else {
        rows.chunks(batch_size).collect()
    };
    let total_batches = batches.len();
    let total_rows = rows.len() as i64;

    // Read any existing checkpoint. A finished run (finished_at set) or a
    // missing row both mean "start fresh".
    let existing: Option<(i64, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "SELECT last_completed_batch_id, finished_at \
         FROM analytics_migration_state WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;

    let resuming = matches!(&existing, Some((_, None)));
    let mut last_completed: i64 = match &existing {
        Some((n, None)) => *n,
        _ => 0,
    };

    if !resuming {
        // Fresh run: wipe the project's ClickHouse rows and reset the
        // checkpoint so resume semantics are clean.
        let pid = pid_uuid(project_id);
        ch.execute(&format!(
            "ALTER TABLE visibility_points DELETE WHERE project_id = '{pid}'"
        ))
        .await?;
        ch.execute(&format!(
            "ALTER TABLE citation_totals DELETE WHERE project_id = '{pid}'"
        ))
        .await?;
        last_completed = 0;
        sqlx::query(
            "INSERT INTO analytics_migration_state \
               (project_id, last_completed_batch_id, batch_size, total_rows_estimate, \
                last_heartbeat_at, started_at, finished_at) \
             VALUES ($1, 0, $2, $3, now(), now(), NULL) \
             ON CONFLICT (project_id) DO UPDATE SET \
               last_completed_batch_id = 0, batch_size = EXCLUDED.batch_size, \
               total_rows_estimate = EXCLUDED.total_rows_estimate, \
               last_heartbeat_at = now(), started_at = now(), finished_at = NULL",
        )
        .bind(project_id)
        .bind(batch_size as i32)
        .bind(total_rows)
        .execute(pool)
        .await?;
    }

    let mut batches_this_run = 0usize;
    for (idx, batch) in batches.iter().enumerate() {
        let batch_id = (idx + 1) as i64; // 1-based
        if batch_id <= last_completed {
            continue; // already committed in a prior invocation
        }

        // Partition the batch by target table and insert.
        let vis: Vec<VisibilityPointRow<'_>> = batch
            .iter()
            .filter_map(|r| match r {
                EtlRow::Visibility {
                    slug,
                    provider,
                    bucket_start,
                    avg_rank,
                    presence_rate,
                } => Some((
                    project_id,
                    slug.as_str(),
                    provider.as_str(),
                    *bucket_start,
                    *avg_rank,
                    *presence_rate,
                )),
                _ => None,
            })
            .collect();
        if !vis.is_empty() {
            ch.seed_visibility_points(&vis).await?;
        }
        let cit: Vec<(ProjectId, &str, i64, Option<&str>)> = batch
            .iter()
            .filter_map(|r| match r {
                EtlRow::Citation {
                    domain,
                    frequency,
                    source_type,
                } => Some((
                    project_id,
                    domain.as_str(),
                    *frequency,
                    source_type.as_deref(),
                )),
                _ => None,
            })
            .collect();
        if !cit.is_empty() {
            ch.seed_citation_totals(&cit).await?;
        }

        // Advance the checkpoint only after the batch fully landed.
        sqlx::query(
            "UPDATE analytics_migration_state \
             SET last_completed_batch_id = $2, last_heartbeat_at = now() \
             WHERE project_id = $1",
        )
        .bind(project_id)
        .bind(batch_id)
        .execute(pool)
        .await?;
        last_completed = batch_id;
        batches_this_run += 1;

        if let Some(stop) = cfg.stop_after_batches {
            if batches_this_run >= stop {
                // Simulated interrupt: leave finished_at NULL so the next run
                // resumes from this checkpoint.
                return Ok(ResumableReport {
                    project_id: Some(project_id),
                    total_batches,
                    batches_completed: last_completed as usize,
                    batches_this_run,
                    finished: false,
                });
            }
        }
    }

    // All batches committed — mark the run finished.
    sqlx::query(
        "UPDATE analytics_migration_state \
         SET finished_at = now(), last_heartbeat_at = now() WHERE project_id = $1",
    )
    .bind(project_id)
    .execute(pool)
    .await?;

    Ok(ResumableReport {
        project_id: Some(project_id),
        total_batches,
        batches_completed: last_completed as usize,
        batches_this_run,
        finished: true,
    })
}
