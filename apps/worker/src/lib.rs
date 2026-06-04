//! `opengeo-worker` library surface.
//!
//! The worker is primarily a binary (`src/main.rs`), but it also exposes a
//! small library so other crates can reach the **ETL enqueue seam** without
//! shelling out:
//!
//! ```ignore
//! let job_id = opengeo_worker::etl::enqueue_etl_job(&pool, project_id).await?;
//! ```
//!
//! Story 31-5's setup handler (`apps/api/src/routes/setup.rs`) calls
//! [`etl::enqueue_etl_job`] to schedule a ClickHouse migration; the worker
//! binary claims and runs those jobs in its poll loop (see [`etl::drain_etl_jobs`]).

pub mod dispatch;
pub mod etl;
pub mod run;

/// Default rolling window (days) for an enqueued ETL run, mirroring the CLI's
/// `migrate-to-clickhouse` default (`apps/cli/src/commands/analytics.rs`).
pub const DEFAULT_ETL_DAYS: i32 = 90;
/// Default top-N citation domains for an enqueued ETL run, mirroring the CLI.
pub const DEFAULT_ETL_CITATION_LIMIT: i64 = 200;
/// Per-sweep cap so a burst of enqueues can't starve the rest of the poll loop.
pub const DEFAULT_ETL_MAX_JOBS_PER_SWEEP: usize = 8;
