//! Story 31-4 — ClickHouse ETL job consumer for the worker.
//!
//! The resumable Postgres→ClickHouse ETL engine
//! ([`opengeo_analytics::metrics_store::clickhouse_etl::migrate_project_resumable`])
//! lives behind the analytics `clickhouse` Cargo feature (it needs `reqwest`).
//! The API process can't link that feature, but the worker can — so the worker
//! owns ETL execution and the API only enqueues.
//!
//! Lifecycle of one enqueued run, all rows in `etl_jobs`:
//! 1. API (story 31-5) calls [`enqueue_etl_job`] → inserts a `pending` row.
//! 2. Worker poll loop calls [`run_pending_etl_jobs`]:
//!    a. [`claim_etl_job`] flips one `pending` row to `running`
//!    at-most-once (`FOR UPDATE SKIP LOCKED`), so two workers racing the
//!    same row don't both run it.
//!    b. The job runner loads the project's declared prompt slugs, builds a
//!    [`ClickHouseMetricsStore`] from env, and calls
//!    `migrate_project_resumable`, which resumes from
//!    `analytics_migration_state.last_completed_batch_id`.
//!    c. [`mark_done`] / [`mark_failed`] records terminal state. A CH connect
//!    failure (e.g. CLICKHOUSE_URL unset / unreachable) is recorded as
//!    `failed` with the error text — the job is not retried automatically.
//!
//! Resume semantics live in `analytics_migration_state`, not here: this table
//! only tracks the lifecycle of a single enqueue request. Re-enqueuing a
//! project after an interrupted run resumes the migration from its checkpoint.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// A claimed ETL job awaiting execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimedEtlJob {
    pub id: Uuid,
    pub project_id: Uuid,
}

/// **31-5 seam.** The setup handler (`apps/api/src/routes/setup.rs`) calls this
/// to schedule a ClickHouse migration for a project after setup completes. It
/// inserts a `pending` row; the worker picks it up on its next poll. Returns
/// the new job id.
///
/// Signature for 31-5 to call:
/// ```ignore
/// let job_id = opengeo_worker::etl::enqueue_etl_job(&pool, project_id).await?;
/// ```
/// where `pool: &sqlx::PgPool` and `project_id: uuid::Uuid` (the project's
/// canonical UUID — convert a `ProjectId` with
/// `uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes())`).
pub async fn enqueue_etl_job(pool: &PgPool, project_id: Uuid) -> Result<Uuid, sqlx::Error> {
    let row = sqlx::query(
        r#"
        INSERT INTO etl_jobs (id, project_id, status, requested_at)
        VALUES ($1, $2, 'pending', now())
        RETURNING id
        "#,
    )
    .bind(Uuid::from_u128(ulid::Ulid::new().0))
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    row.try_get("id")
}

/// Claim the oldest `pending` ETL job at-most-once, flipping it to `running`.
///
/// Uses `FOR UPDATE SKIP LOCKED` so two workers racing the queue each grab a
/// distinct row (or `None` when the queue is drained) without blocking on each
/// other's locks. Returns `None` when there is nothing to claim.
pub async fn claim_etl_job(pool: &PgPool) -> Result<Option<ClaimedEtlJob>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        WITH next AS (
            SELECT id
            FROM etl_jobs
            WHERE status = 'pending'
            ORDER BY requested_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        UPDATE etl_jobs
        SET status = 'running', started_at = now()
        FROM next
        WHERE etl_jobs.id = next.id
        RETURNING etl_jobs.id, etl_jobs.project_id
        "#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(r) => Some(ClaimedEtlJob {
            id: r.try_get("id")?,
            project_id: r.try_get("project_id")?,
        }),
        None => None,
    })
}

/// Mark a claimed job `done`.
pub async fn mark_done(pool: &PgPool, job_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE etl_jobs
        SET status = 'done', finished_at = now(), error = NULL
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a claimed job `failed`, recording the error text (truncated for sanity).
pub async fn mark_failed(pool: &PgPool, job_id: Uuid, error: &str) -> Result<(), sqlx::Error> {
    let truncated: String = error.chars().take(2000).collect();
    sqlx::query(
        r#"
        UPDATE etl_jobs
        SET status = 'failed', finished_at = now(), error = $2
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .bind(truncated)
    .execute(pool)
    .await?;
    Ok(())
}

/// Inspect a job's current status/error — used by tests and operators.
pub async fn job_status(
    pool: &PgPool,
    job_id: Uuid,
) -> Result<Option<(String, Option<String>, Option<DateTime<Utc>>)>, sqlx::Error> {
    let row = sqlx::query(r#"SELECT status, error, finished_at FROM etl_jobs WHERE id = $1"#)
        .bind(job_id)
        .fetch_optional(pool)
        .await?;
    Ok(match row {
        Some(r) => Some((
            r.try_get("status")?,
            r.try_get("error")?,
            r.try_get("finished_at")?,
        )),
        None => None,
    })
}

/// Drain pending ETL jobs: claim one, run it via `runner`, record terminal
/// state. Repeats until the queue is empty or `max_jobs` is reached, so a burst
/// of enqueues doesn't wait one poll interval per job.
///
/// `runner` is the execution seam: production passes [`run_migration`] (which
/// needs a live ClickHouse); tests pass a stub that asserts the claim/transition
/// path without standing up ClickHouse.
pub async fn drain_etl_jobs<F, Fut>(
    pool: &PgPool,
    max_jobs: usize,
    runner: F,
) -> Result<usize, sqlx::Error>
where
    F: Fn(ClaimedEtlJob) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let mut processed = 0usize;
    while processed < max_jobs {
        let Some(job) = claim_etl_job(pool).await? else {
            break;
        };
        match runner(job).await {
            Ok(()) => mark_done(pool, job.id).await?,
            Err(err) => mark_failed(pool, job.id, &err).await?,
        }
        processed += 1;
    }
    Ok(processed)
}

/// Production ETL runner: load the project's declared prompt slugs, build the
/// ClickHouse store from env, and run the resumable migration (resuming from
/// `analytics_migration_state.last_completed_batch_id`). Available only with
/// the analytics `clickhouse` feature; without it the runner reports a clear
/// error so the job lands in `failed` rather than failing to compile.
#[cfg(feature = "clickhouse")]
pub async fn run_migration(
    pool: &PgPool,
    job: ClaimedEtlJob,
    days: i32,
    citation_limit: i64,
) -> Result<(), String> {
    use opengeo_analytics::metrics_store::clickhouse::ClickHouseMetricsStore;
    use opengeo_analytics::metrics_store::clickhouse_etl::{
        migrate_project_resumable, ResumableConfig,
    };
    use opengeo_core::ids::ProjectId;
    use opengeo_storage::Storage;

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set".to_string())?;
    // Wrap the shared pool in a Storage handle so we reuse the repos. A fresh
    // connect keeps the runner self-contained; the pool above is used for the
    // queue-state transitions in `drain_etl_jobs`.
    let _ = pool;
    let storage = Storage::connect(&database_url)
        .await
        .map_err(|e| format!("failed to connect to Postgres: {e}"))?;

    let project_id = ProjectId::from_ulid(ulid::Ulid::from_bytes(*job.project_id.as_bytes()));

    let prompts = storage
        .prompts()
        .list_by_project(project_id)
        .await
        .map_err(|e| format!("failed to load project prompts: {e}"))?;
    let prompt_slugs: Vec<String> = prompts.into_iter().map(|p| p.name).collect();
    if prompt_slugs.is_empty() {
        return Err("project has no declared prompts to migrate".to_string());
    }
    let slug_refs: Vec<&str> = prompt_slugs.iter().map(String::as_str).collect();

    let ch = ClickHouseMetricsStore::from_env().map_err(|_| {
        "CLICKHOUSE_URL, CLICKHOUSE_USER, CLICKHOUSE_PASSWORD, CLICKHOUSE_DATABASE must be set"
            .to_string()
    })?;

    migrate_project_resumable(
        &storage,
        &ch,
        project_id,
        &slug_refs,
        days,
        citation_limit,
        &ResumableConfig::default(),
    )
    .await
    .map_err(|e| format!("ETL failed: {e}"))?;
    Ok(())
}

/// Queue-only fallback when the worker is built `--no-default-features` (no
/// `clickhouse`). The job is still claimed and lands in `failed` with a clear
/// message rather than failing to compile or silently no-op'ing.
#[cfg(not(feature = "clickhouse"))]
pub async fn run_migration(
    pool: &PgPool,
    job: ClaimedEtlJob,
    days: i32,
    citation_limit: i64,
) -> Result<(), String> {
    let _ = (pool, job, days, citation_limit);
    Err(
        "worker built without the `clickhouse` feature — rebuild with \
         `--features clickhouse` to run the ClickHouse ETL"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    async fn insert_project(pool: &PgPool) -> Uuid {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"INSERT INTO projects (id, name, organization_id, tenant_id, created_at)
               VALUES ($1, $2, NULL, NULL, now())"#,
        )
        .bind(id)
        .bind(format!("proj-{}", &id.to_string()[..8]))
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn enqueue_inserts_pending_job(pool: PgPool) {
        let project_id = insert_project(&pool).await;
        let job_id = enqueue_etl_job(&pool, project_id).await.unwrap();
        let (status, error, finished) = job_status(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(status, "pending");
        assert!(error.is_none());
        assert!(finished.is_none());
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn claim_transitions_pending_to_running_once(pool: PgPool) {
        let project_id = insert_project(&pool).await;
        let job_id = enqueue_etl_job(&pool, project_id).await.unwrap();

        let claimed = claim_etl_job(&pool).await.unwrap().expect("a pending job");
        assert_eq!(claimed.id, job_id);
        assert_eq!(claimed.project_id, project_id);

        let (status, _, _) = job_status(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(status, "running");

        // No more pending jobs: a second claim returns None (at-most-once).
        assert!(claim_etl_job(&pool).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn drain_marks_done_on_success(pool: PgPool) {
        let project_id = insert_project(&pool).await;
        let job_id = enqueue_etl_job(&pool, project_id).await.unwrap();

        let seen = Arc::new(AtomicUsize::new(0));
        let seen_c = seen.clone();
        // Stub runner: succeeds without touching ClickHouse.
        let processed = drain_etl_jobs(&pool, 10, move |job| {
            let seen = seen_c.clone();
            async move {
                assert_eq!(job.project_id, project_id);
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await
        .unwrap();

        assert_eq!(processed, 1);
        assert_eq!(seen.load(Ordering::SeqCst), 1);
        let (status, error, finished) = job_status(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(status, "done");
        assert!(error.is_none());
        assert!(finished.is_some());
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn drain_records_failure_gracefully(pool: PgPool) {
        // Models the no-live-ClickHouse path: the runner errors (as the real
        // CH connect would), and the job lands in `failed` with the message.
        let project_id = insert_project(&pool).await;
        let job_id = enqueue_etl_job(&pool, project_id).await.unwrap();

        let processed = drain_etl_jobs(&pool, 10, |_job| async {
            Err("CLICKHOUSE_URL ... must be set".to_string())
        })
        .await
        .unwrap();

        assert_eq!(processed, 1);
        let (status, error, finished) = job_status(&pool, job_id).await.unwrap().unwrap();
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some("CLICKHOUSE_URL ... must be set"));
        assert!(finished.is_some());
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn drain_processes_multiple_in_one_sweep(pool: PgPool) {
        let p1 = insert_project(&pool).await;
        let p2 = insert_project(&pool).await;
        enqueue_etl_job(&pool, p1).await.unwrap();
        enqueue_etl_job(&pool, p2).await.unwrap();

        let processed = drain_etl_jobs(&pool, 10, |_job| async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(processed, 2);

        // Queue drained.
        assert!(claim_etl_job(&pool).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn drain_respects_max_jobs(pool: PgPool) {
        let p1 = insert_project(&pool).await;
        let p2 = insert_project(&pool).await;
        enqueue_etl_job(&pool, p1).await.unwrap();
        enqueue_etl_job(&pool, p2).await.unwrap();

        let processed = drain_etl_jobs(&pool, 1, |_job| async { Ok(()) })
            .await
            .unwrap();
        assert_eq!(processed, 1);
        // One job remains claimable.
        assert!(claim_etl_job(&pool).await.unwrap().is_some());
    }
}
