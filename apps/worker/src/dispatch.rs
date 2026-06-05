//! Story 36.4 — worker multi-project fan-out.
//!
//! The worker used to dispatch schedules + ETL against a single boot-pinned
//! project (its brand overlay was loaded once at startup via
//! `get_single_brand`). This module fans the per-tick work out across **every
//! active (non-archived) project** instead:
//!
//! 1. [`fan_out_dispatch`] re-reads [`ProjectRepo::list_projects`] each tick, so
//!    projects created or archived mid-run are picked up / dropped on the next
//!    tick (no restart required).
//! 2. Each project's schedule-dispatch sweep is scoped to that project's
//!    `project_id` and run against a `Config` whose brand overlay is rebuilt
//!    from that project's DB row — so mention/citation extraction scores against
//!    the right brand.
//! 3. **Fault isolation + fairness.** Each project's sweep runs in its own
//!    `tokio` task gated by a [`Semaphore`] that bounds per-tick concurrency
//!    (reserving headroom so one project can't monopolise the runtime). A task
//!    that panics or errors is contained at the join boundary — it never blocks
//!    or crashes the others.
//! 4. Per-project last-run-age is surfaced as a structured log line each tick.
//!
//! The ETL sweep is already multi-project-correct (jobs carry their own
//! `project_id` and are claimed globally), so it stays in the main loop; only
//! schedule dispatch needs per-project brand scoping.

use std::collections::HashMap;
use std::sync::Arc;

use anseo_core::Config;
use anseo_providers::ProviderRegistry;
use anseo_scheduler::dispatch::dispatch_due_schedules_scoped;
use anseo_scheduler::events::LifecycleEvent;
use anseo_scheduler::worker::WorkerError;
use anseo_storage::Storage;
use chrono::{DateTime, Utc};
use tokio::sync::Semaphore;
use uuid::Uuid;

/// Default ceiling on how many projects dispatch concurrently within a single
/// tick. Bounded so a deployment with many projects can't spawn an unbounded
/// task fan-out and starve the rest of the poll loop (reaper, webhooks, ETL).
pub const DEFAULT_MAX_PROJECT_CONCURRENCY: usize = 4;

/// Outcome of one project's dispatch sweep within a tick.
#[derive(Debug)]
pub enum ProjectOutcome {
    /// The sweep ran (possibly dispatching zero ticks) and returned events.
    Ok(Vec<LifecycleEvent>),
    /// The sweep returned a [`WorkerError`]; contained, others continue.
    Failed(WorkerError),
    /// The sweep panicked; contained at the join boundary, others continue.
    Panicked(String),
}

/// Per-project fan-out report for one tick. Carries the events the caller must
/// publish + fan out (already flattened across projects) plus the per-project
/// last-run-age the caller logs / exports as a metric.
#[derive(Debug, Default)]
pub struct FanOutReport {
    /// Lifecycle events to publish (NOTIFY) + enqueue (webhooks), across all
    /// projects that dispatched this tick.
    pub events: Vec<LifecycleEvent>,
    /// Count of projects whose sweep failed (error or panic) this tick.
    pub failed_projects: usize,
    /// Count of projects whose sweep completed (no error/panic) this tick.
    pub ok_projects: usize,
}

/// Build a per-project [`Config`] by overlaying the project's DB brand row onto
/// `base_config`. Mirrors the boot-time overlay the worker used to do once via
/// `get_single_brand`, but resolved per project so each tick scores against the
/// correct brand. Returns `base_config.clone()` unchanged when the project has
/// no brand row (defaults), so dispatch still proceeds.
async fn project_config(
    storage: &Storage,
    base_config: &Config,
    project_id: anseo_core::ProjectId,
) -> Result<Config, WorkerError> {
    let mut cfg = base_config.clone();
    if let Some(brand) = storage.projects().get_brand(project_id).await? {
        cfg.brand.name = brand.name.clone();
        cfg.brand.variants = brand.variants.clone();
        cfg.competitors = serde_json::from_value(brand.competitors).unwrap_or_default();
    }
    Ok(cfg)
}

/// Fan the schedule-dispatch sweep out across every active project.
///
/// Re-reads the active project list, then runs each project's scoped dispatch
/// in a bounded, fault-isolated task. Returns a [`FanOutReport`] whose `events`
/// the caller publishes; per-project last-run-age is logged here.
///
/// `base_config` / `registry` are deployment-wide (provider keys come from the
/// boot YAML, not per project); only the brand overlay differs per project.
pub async fn fan_out_dispatch(
    storage: &Storage,
    base_config: &Config,
    registry: &ProviderRegistry,
    claimed_by: &str,
    now: DateTime<Utc>,
    max_concurrency: usize,
) -> Result<FanOutReport, WorkerError> {
    let projects = storage.projects().list_projects().await?;

    if projects.is_empty() {
        tracing::debug!(
            event = "worker.fanout.no_projects",
            "no active projects found; schedule dispatch skipped this tick"
        );
        return Ok(FanOutReport::default());
    }

    // Per-project last-run-age surfacing: the age of each project's most recent
    // schedule tick (any status) as of `now`. Read once up front so a panic in
    // a dispatch task can't suppress the observability line.
    let last_run_ages = last_run_ages(storage, now).await;

    // Build one dispatch future per project, each scoped to that project's
    // brand-overlaid config. Config-build errors are folded into the future as a
    // contained `Failed` so the project still appears in the report.
    let mut units: Vec<ProjectUnit> = Vec::with_capacity(projects.len());
    for project in &projects {
        let project_id = project.id;
        let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let project_name = project.name.clone();

        let cfg = project_config(storage, base_config, project_id).await;
        // `Storage` isn't `Clone`, but its `PgPool` is (Arc-backed); rebuild a
        // per-task `Storage` from the shared pool so each project's sweep owns
        // its handle without contending on a borrow.
        let task_storage = Storage::from_pool(storage.pool().clone());
        let registry = registry.clone();
        let claimed_by = claimed_by.to_string();

        let fut = async move {
            let cfg = cfg?;
            dispatch_due_schedules_scoped(
                task_storage.pool(),
                &task_storage,
                &cfg,
                &registry,
                Some(project_uuid),
                &claimed_by,
                now,
            )
            .await
        };
        units.push(ProjectUnit {
            name: project_name,
            fut: Box::pin(fut),
        });
    }

    let outcomes = run_isolated(units, max_concurrency).await;

    let mut report = FanOutReport::default();
    for (project_name, outcome) in outcomes {
        let age = last_run_ages.get(&project_name).copied().flatten();
        match outcome {
            ProjectOutcome::Ok(mut events) => {
                report.ok_projects += 1;
                let dispatched = events.len();
                report.events.append(&mut events);
                tracing::info!(
                    event = "worker.fanout.project_swept",
                    project = %project_name,
                    dispatched_events = dispatched,
                    last_run_age_seconds = age,
                    "project schedule sweep complete"
                );
            }
            ProjectOutcome::Failed(err) => {
                report.failed_projects += 1;
                tracing::warn!(
                    event = "worker.fanout.project_failed",
                    project = %project_name,
                    error = %err,
                    last_run_age_seconds = age,
                    "project schedule sweep failed; other projects unaffected"
                );
            }
            ProjectOutcome::Panicked(msg) => {
                report.failed_projects += 1;
                tracing::error!(
                    event = "worker.fanout.project_panicked",
                    project = %project_name,
                    panic = %msg,
                    last_run_age_seconds = age,
                    "project schedule sweep panicked; other projects unaffected"
                );
            }
        }
    }

    Ok(report)
}

/// One project's named dispatch future, ready to be run under [`run_isolated`].
struct ProjectUnit {
    name: String,
    fut: std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<LifecycleEvent>, WorkerError>> + Send>,
    >,
}

/// Run each project's future in its own `tokio` task, bounding concurrency to
/// `max_concurrency` permits and **isolating faults**: a future that errors
/// becomes [`ProjectOutcome::Failed`] and one that *panics* is caught at the
/// join boundary as [`ProjectOutcome::Panicked`] — neither blocks nor aborts
/// the siblings (they run in independent tasks). Returns one named outcome per
/// unit, in input order.
async fn run_isolated(
    units: Vec<ProjectUnit>,
    max_concurrency: usize,
) -> Vec<(String, ProjectOutcome)> {
    let permits = Arc::new(Semaphore::new(max_concurrency.max(1)));
    let mut handles = Vec::with_capacity(units.len());

    for unit in units {
        let permits = Arc::clone(&permits);
        let name = unit.name;
        let fut = unit.fut;
        let handle = tokio::spawn(async move {
            // Bound concurrency: wait for a permit before running. A closed
            // semaphore means the runtime is shutting down; treat as a no-op.
            let _permit = match permits.acquire().await {
                Ok(p) => p,
                Err(_) => return ProjectOutcome::Ok(Vec::new()),
            };
            match fut.await {
                Ok(events) => ProjectOutcome::Ok(events),
                Err(err) => ProjectOutcome::Failed(err),
            }
        });
        handles.push((name, handle));
    }

    let mut out = Vec::with_capacity(handles.len());
    for (name, handle) in handles {
        let outcome = match handle.await {
            Ok(o) => o,
            // A panic inside the spawned task lands here, contained: the join
            // error does NOT propagate, so siblings (their own tasks) are
            // unaffected.
            Err(join_err) => ProjectOutcome::Panicked(join_err.to_string()),
        };
        out.push((name, outcome));
    }
    out
}

/// Compute each project's last-run-age (seconds since its most recent schedule
/// tick, any status) keyed by project name. `None` for a project that has
/// never ticked. Best-effort: a read error yields an empty map (observability
/// only — it must not affect dispatch).
async fn last_run_ages(storage: &Storage, now: DateTime<Utc>) -> HashMap<String, Option<i64>> {
    let rows = sqlx::query!(
        r#"
        SELECT
            p.name AS name,
            (
                SELECT MAX(t.tick_ts)
                FROM schedule_ticks t
                JOIN schedules s ON s.id = t.schedule_id
                WHERE s.project_id = p.id
            ) AS "last_tick_ts?"
        FROM projects p
        WHERE p.archived_at IS NULL
        "#,
    )
    .fetch_all(storage.pool())
    .await;

    match rows {
        Ok(rows) => rows
            .into_iter()
            .map(|r| {
                let age = r
                    .last_tick_ts
                    .map(|t: DateTime<Utc>| (now - t).num_seconds());
                (r.name, age)
            })
            .collect(),
        Err(err) => {
            tracing::warn!(
                event = "worker.fanout.last_run_age_failed",
                error = %err,
                "failed to read per-project last-run-age; continuing without it"
            );
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_core::{ProjectId, ProviderName};
    use anseo_providers::MockProvider;
    use sqlx::PgPool;
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // --- Fault-isolation / fairness: pure `run_isolated` tests ---------------

    /// A panicking project unit must NOT stop the others: every sibling still
    /// runs to completion and the panicker is reported as `Panicked`.
    #[tokio::test]
    async fn panicking_project_does_not_stop_others() {
        let ran = Arc::new(AtomicUsize::new(0));

        let mut units = Vec::new();
        for i in 0..3usize {
            let ran = Arc::clone(&ran);
            let name = format!("proj-{i}");
            let fut: std::pin::Pin<Box<dyn std::future::Future<Output = _> + Send>> = if i == 1 {
                // The middle project panics mid-flight.
                Box::pin(async move {
                    panic!("boom in proj-1");
                })
            } else {
                Box::pin(async move {
                    ran.fetch_add(1, Ordering::SeqCst);
                    Ok(Vec::new())
                })
            };
            units.push(ProjectUnit { name, fut });
        }

        let outcomes = run_isolated(units, 4).await;

        // All three projects are accounted for, in order.
        assert_eq!(outcomes.len(), 3);
        assert_eq!(outcomes[0].0, "proj-0");
        assert_eq!(outcomes[1].0, "proj-1");
        assert_eq!(outcomes[2].0, "proj-2");

        // The two healthy projects ran to completion despite the panic.
        assert_eq!(ran.load(Ordering::SeqCst), 2, "both healthy projects ran");
        assert!(matches!(outcomes[0].1, ProjectOutcome::Ok(_)));
        assert!(matches!(outcomes[2].1, ProjectOutcome::Ok(_)));
        // The panicker is contained and reported.
        assert!(matches!(outcomes[1].1, ProjectOutcome::Panicked(_)));
    }

    /// Concurrency is bounded: with a single permit, at most one project runs
    /// at a time (peak in-flight never exceeds the bound), yet all still run.
    #[tokio::test]
    async fn concurrency_is_bounded() {
        let in_flight = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));

        let mut units = Vec::new();
        for i in 0..5usize {
            let in_flight = Arc::clone(&in_flight);
            let peak = Arc::clone(&peak);
            let fut: std::pin::Pin<Box<dyn std::future::Future<Output = _> + Send>> =
                Box::pin(async move {
                    let cur = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(cur, Ordering::SeqCst);
                    tokio::task::yield_now().await;
                    in_flight.fetch_sub(1, Ordering::SeqCst);
                    Ok(Vec::new())
                });
            units.push(ProjectUnit {
                name: format!("proj-{i}"),
                fut,
            });
        }

        let outcomes = run_isolated(units, 1).await;
        assert_eq!(outcomes.len(), 5);
        assert!(outcomes
            .iter()
            .all(|(_, o)| matches!(o, ProjectOutcome::Ok(_))));
        assert_eq!(
            peak.load(Ordering::SeqCst),
            1,
            "max_concurrency=1 must serialize the units"
        );
    }

    // --- Multi-project dispatch: live-DB tests -------------------------------

    fn test_config() -> Config {
        let yaml = r#"
schema_version: "0.1"
brand:
  name: "Base Brand"
prompts:
  - name: placeholder
    text: placeholder text
"#;
        Config::from_yaml_str(yaml).expect("valid test config")
    }

    fn mock_registry() -> ProviderRegistry {
        let mut reg: ProviderRegistry = std::collections::HashMap::new();
        // Queue plenty of responses; the single shared mock serves every
        // project's tick. An exhausted queue degrades to a failed run (still a
        // completed tick + a persisted prompt_runs row), so the count is not
        // load-bearing for these assertions.
        let mut mock = MockProvider::new(ProviderName::Openai);
        for i in 0..16 {
            mock = mock.queue_response(format!("mock response {i}"));
        }
        reg.insert(ProviderName::Openai, Arc::new(mock));
        reg
    }

    /// Seed a project + one prompt + one due (hourly, far-past-anchored)
    /// schedule. Returns the project's UUID.
    async fn seed_project_with_due_schedule(pool: &PgPool, label: &str) -> Uuid {
        let project_id = ProjectId::new();
        let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
        sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
            .bind(project_uuid)
            .bind(format!("brand-{label}"))
            .execute(pool)
            .await
            .unwrap();

        let prompt_id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"INSERT INTO prompts (id, project_id, name, text, created_at)
               VALUES ($1, $2, $3, $4, now())"#,
        )
        .bind(prompt_id)
        .bind(project_uuid)
        .bind("watch-prompt")
        .bind("what do you think of us?")
        .execute(pool)
        .await
        .unwrap();

        // created_at far in the past so the hourly anchor is well before `now`
        // and the next tick is due.
        let schedule_id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"INSERT INTO schedules
               (id, project_id, name, cron, prompts, providers, paused, created_at)
               VALUES ($1, $2, $3, 'hourly', $4, $5, FALSE, now() - interval '2 days')"#,
        )
        .bind(schedule_id)
        .bind(project_uuid)
        .bind(format!("sched-{label}"))
        .bind(serde_json::json!(["watch-prompt"]))
        .bind(serde_json::json!(["openai"]))
        .execute(pool)
        .await
        .unwrap();

        project_uuid
    }

    async fn completed_tick_projects(pool: &PgPool) -> HashSet<Uuid> {
        let rows = sqlx::query_as::<_, (Uuid,)>(
            r#"SELECT DISTINCT s.project_id
               FROM schedule_ticks t
               JOIN schedules s ON s.id = t.schedule_id
               WHERE t.status = 'completed'"#,
        )
        .fetch_all(pool)
        .await
        .unwrap();
        rows.into_iter().map(|(p,)| p).collect()
    }

    /// AC: a multi-project tick dispatches every active project — ≥2 projects
    /// each get a completed schedule tick in a single `fan_out_dispatch` call.
    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn fan_out_dispatches_every_active_project(pool: PgPool) {
        let p_a = seed_project_with_due_schedule(&pool, "a").await;
        let p_b = seed_project_with_due_schedule(&pool, "b").await;
        let p_c = seed_project_with_due_schedule(&pool, "c").await;

        let storage = Storage::from_pool(pool.clone());
        let report = fan_out_dispatch(
            &storage,
            &test_config(),
            &mock_registry(),
            "test-worker",
            Utc::now(),
            DEFAULT_MAX_PROJECT_CONCURRENCY,
        )
        .await
        .expect("fan-out should not error");

        // Every project's sweep completed (none failed/panicked).
        assert_eq!(report.failed_projects, 0);
        assert_eq!(report.ok_projects, 3);

        let dispatched = completed_tick_projects(&pool).await;
        assert!(
            dispatched.len() >= 2,
            "at least two projects must be dispatched in a tick, got {}",
            dispatched.len()
        );
        for p in [p_a, p_b, p_c] {
            assert!(dispatched.contains(&p), "project {p} must be dispatched");
        }
    }

    /// AC: archiving a project mid-run drops it from the next tick; a freshly
    /// created project is picked up — the list is re-read each call.
    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn fan_out_re_reads_active_projects_each_tick(pool: PgPool) {
        let p_a = seed_project_with_due_schedule(&pool, "a").await;
        let p_b = seed_project_with_due_schedule(&pool, "b").await;
        let storage = Storage::from_pool(pool.clone());

        // Archive A before the tick: it must be skipped.
        sqlx::query("UPDATE projects SET archived_at = now() WHERE id = $1")
            .bind(p_a)
            .execute(&pool)
            .await
            .unwrap();

        let report = fan_out_dispatch(
            &storage,
            &test_config(),
            &mock_registry(),
            "test-worker",
            Utc::now(),
            DEFAULT_MAX_PROJECT_CONCURRENCY,
        )
        .await
        .expect("fan-out should not error");

        // Only the one active project (B) was swept.
        assert_eq!(report.ok_projects, 1);
        let dispatched = completed_tick_projects(&pool).await;
        assert!(dispatched.contains(&p_b), "active project B dispatched");
        assert!(
            !dispatched.contains(&p_a),
            "archived project A must be skipped"
        );
    }
}
