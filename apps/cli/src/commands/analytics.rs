//! `ogeo analytics migrate-to-clickhouse` — Phase 2 Story 14.1 CLI.
//!
//! Drains the current project's Postgres analytics views into the
//! ClickHouse pre-aggregated tables. Idempotent: re-running clears the
//! project's prior rows before re-inserting, so partial runs converge
//! to a clean state on the next invocation. Resumable / checkpointed
//! batched migration with the `analytics_migration_state` table is a
//! follow-up; this minimal verb covers the "fresh ClickHouse → primed"
//! day-one case.

use std::path::PathBuf;

use anseo_core::OpenGeoError;
use anseo_storage::Storage;
use clap::Args;

const DEFAULT_DAYS: i32 = 90;
const DEFAULT_CITATION_LIMIT: i64 = 200;

#[derive(Debug, Args)]
pub struct MigrateArgs {
    /// Path to opengeo.yaml. Defaults to `./opengeo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Rolling window (days) of visibility points to migrate per
    /// declared prompt. Clamped to [1, 365].
    #[arg(long, default_value_t = DEFAULT_DAYS)]
    pub days: i32,
    /// Top-N domains to migrate to citation_totals. Clamped to [1, 500].
    #[arg(long, default_value_t = DEFAULT_CITATION_LIMIT)]
    pub citation_limit: i64,
}

pub async fn run_migrate(args: MigrateArgs) -> Result<(), OpenGeoError> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| OpenGeoError::Config("DATABASE_URL must be set".into()))?;
    let path = args
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("anseo.yaml"));
    let cfg = anseo_core::Config::from_path(&path).map_err(|e| {
        OpenGeoError::Config(format!(
            "failed to read project config at `{}`: {e}",
            path.display()
        ))
    })?;
    let project_id = cfg.project_id();
    let prompt_slugs: Vec<&str> = cfg.prompts.iter().map(|p| p.name.as_str()).collect();
    if prompt_slugs.is_empty() {
        return Err(OpenGeoError::Config(
            "no prompts declared in this project — declare one with `ogeo prompt add` first".into(),
        ));
    }

    let storage = Storage::connect(&database_url)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to connect to Postgres: {e}")))?;

    #[cfg(feature = "clickhouse")]
    {
        use anseo_analytics::metrics_store::clickhouse::ClickHouseMetricsStore;
        use anseo_analytics::metrics_store::clickhouse_etl::{
            migrate_project_resumable, ResumableConfig,
        };
        // Apply forward-only migrations so the `analytics_migration_state`
        // checkpoint table exists before the resumable ETL touches it.
        storage
            .migrate()
            .await
            .map_err(|e| OpenGeoError::Config(format!("migration failed: {e}")))?;
        let ch = ClickHouseMetricsStore::from_env().map_err(|_| {
            OpenGeoError::Config(
                "CLICKHOUSE_URL, CLICKHOUSE_USER, CLICKHOUSE_PASSWORD, CLICKHOUSE_DATABASE must be set".into(),
            )
        })?;
        let report = migrate_project_resumable(
            &storage,
            &ch,
            project_id,
            &prompt_slugs,
            args.days,
            args.citation_limit,
            &ResumableConfig::default(),
        )
        .await
        .map_err(|e| OpenGeoError::Config(format!("ETL failed: {e}")))?;
        // Interrupt-safe: a SIGTERM mid-run leaves `finished=false`; the next
        // invocation resumes from `last_completed_batch_id`.
        println!(
            "✓ ClickHouse ETL for project {project_id}\n\
             - batches: {}/{} completed\n\
             - status: {}",
            report.batches_completed,
            report.total_batches,
            if report.finished {
                "finished"
            } else {
                "interrupted (resume to continue)"
            }
        );
        Ok(())
    }
    #[cfg(not(feature = "clickhouse"))]
    {
        let _ = (storage, project_id, prompt_slugs, args);
        Err(OpenGeoError::Config(
            "rebuild this CLI with `--features clickhouse` to enable the migration verb".into(),
        ))
    }
}
