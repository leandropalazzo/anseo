//! Reusable poll-loop entrypoint for the background worker.
//!
//! The worker binary (`src/main.rs`) is a thin wrapper that wires telemetry +
//! signal handling and then calls [`run_poll_loop`]. The same loop is reused
//! **in-process** by `ogeo serve` (Story 37.1), which boots the HTTP API and
//! the worker in a single binary and drives the loop as a supervised tokio
//! task. Hoisting the loop into the library is what lets both the standalone
//! `anseo-worker` binary and the folded `ogeo serve` supervisor share one
//! implementation instead of duplicating the dispatch/reaper/webhook/ETL sweep.

use std::future::Future;
use std::time::Duration;

use anseo_scheduler::events::{LifecycleEvent, SchedulePayload};
use anseo_scheduler::transport::publish;
use anseo_scheduler::webhooks::fanout::enqueue_lifecycle_event;
use anseo_scheduler::webhooks::poller::{poll_once, DEFAULT_BATCH_LIMIT, DEFAULT_DELIVERY_TIMEOUT};
use anseo_scheduler::webhooks::tick::DispatchResult;
use anseo_scheduler::worker::reap_orphans;
use anseo_storage::Storage;
use uuid::Uuid;

use crate::etl::{drain_etl_jobs, run_migration};
use crate::{DEFAULT_ETL_CITATION_LIMIT, DEFAULT_ETL_DAYS, DEFAULT_ETL_MAX_JOBS_PER_SWEEP};

/// Seconds between poll sweeps. Mirrors the legacy binary constant.
pub const POLL_INTERVAL_SECONDS: u64 = 5;

/// Deployment-wide substrate the per-tick fan-out clones per project: the base
/// `Config` (provider keys + concurrency) and the live `ProviderRegistry`.
/// When `None`, schedule dispatch is inert (reaper + webhooks + ETL still run),
/// matching the binary's "no readable config / registry" degraded mode.
pub struct DispatchContext {
    pub config: anseo_core::Config,
    pub registry: anseo_providers::ProviderRegistry,
    pub worker_id: String,
}

/// Run the worker poll loop until `shutdown` resolves.
///
/// Each tick reaps abandoned ticks, fans schedule dispatch out across every
/// active project (bounded + fault-isolated; see [`crate::dispatch`]), drains
/// the webhook queue, and drains enqueued ClickHouse ETL jobs. Every sweep is
/// individually error-contained so one failing subsystem never wedges the rest
/// or the loop. Returns once `shutdown` fires (graceful) — the caller is
/// responsible for installing the signal source (the standalone binary uses
/// SIGINT/SIGTERM; `ogeo serve` shares one shutdown broadcast across the API
/// and worker).
pub async fn run_poll_loop(
    storage: &Storage,
    dispatch: Option<&DispatchContext>,
    shutdown: impl Future<Output = ()>,
) -> anyhow::Result<()> {
    let pool = storage.pool().clone();

    let http_client = reqwest::Client::builder()
        .user_agent("opengeo-webhook-dispatcher/0.1")
        .build()?;

    tokio::pin!(shutdown);

    let mut poll = tokio::time::interval(Duration::from_secs(POLL_INTERVAL_SECONDS));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    tracing::info!(event = "worker.ready", "worker ready; entering poll loop");

    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => {
                tracing::info!(event = "service.shutdown", "shutdown signal received");
                break;
            }
            _ = poll.tick() => {
                run_tick(storage, &pool, dispatch, &http_client).await;
            }
        }
    }

    Ok(())
}

/// Body of one poll sweep, extracted so the loop above stays readable. All
/// errors are contained here (logged + retried next tick).
async fn run_tick(
    storage: &Storage,
    pool: &sqlx::PgPool,
    dispatch: Option<&DispatchContext>,
    http_client: &reqwest::Client,
) {
    // Reaper sweep: roll forward abandoned `claimed` ticks past the idle window.
    match reap_orphans(pool).await {
        Ok(reaped) => {
            for tick in &reaped {
                let payload = SchedulePayload {
                    event_id: Uuid::from_u128(ulid::Ulid::new().0),
                    project_id: tick.project_id,
                    schedule_id: tick.schedule_id,
                    schedule_name: tick.schedule_name.clone(),
                    tick_id: tick.tick_id,
                    tick_ts: tick.tick_ts,
                    emitted_at: chrono::Utc::now(),
                };
                let event = LifecycleEvent::TickRolledForward(payload);
                if let Err(err) = publish(pool, &event).await {
                    tracing::warn!(
                        event = "worker.publish_failed",
                        error = %err,
                        "failed to publish rolled-forward event"
                    );
                }
                if let Err(err) = enqueue_lifecycle_event(storage, &event).await {
                    tracing::warn!(
                        event = "worker.fanout_failed",
                        error = %err,
                        "failed to enqueue webhook deliveries"
                    );
                }
            }
            if !reaped.is_empty() {
                tracing::info!(
                    event = "worker.reaper_swept",
                    count = reaped.len(),
                    "reaped abandoned ticks"
                );
            }
        }
        Err(err) => tracing::warn!(
            event = "worker.reaper_failed",
            error = %err,
            "reaper sweep failed; will retry on next tick"
        ),
    }

    // Story 36.4 — multi-project fan-out. Re-read the active project list each
    // tick and dispatch every project's due ticks scoped to its own brand
    // overlay, in bounded fault-isolated tasks. Inert when config/registry were
    // unavailable at boot.
    if let Some(ctx) = dispatch {
        match crate::dispatch::fan_out_dispatch(
            storage,
            &ctx.config,
            &ctx.registry,
            &ctx.worker_id,
            chrono::Utc::now(),
            crate::dispatch::DEFAULT_MAX_PROJECT_CONCURRENCY,
        )
        .await
        {
            Ok(report) => {
                for event in &report.events {
                    if let Err(err) = publish(pool, event).await {
                        tracing::warn!(
                            event = "worker.publish_failed",
                            error = %err,
                            "failed to publish schedule dispatch event"
                        );
                    }
                    if let Err(err) = enqueue_lifecycle_event(storage, event).await {
                        tracing::warn!(
                            event = "worker.fanout_failed",
                            error = %err,
                            "failed to enqueue webhook deliveries for dispatch event"
                        );
                    }
                }
                if report.failed_projects > 0 {
                    tracing::warn!(
                        event = "worker.fanout.partial",
                        ok_projects = report.ok_projects,
                        failed_projects = report.failed_projects,
                        "some project sweeps failed this tick; others proceeded"
                    );
                }
            }
            Err(err) => tracing::warn!(
                event = "worker.dispatch_failed",
                error = %err,
                "project fan-out sweep failed; will retry on next tick"
            ),
        }
    }

    // Story 12.4: drive the webhook dispatcher in the same tick.
    match poll_once(
        storage,
        http_client,
        DEFAULT_BATCH_LIMIT,
        DEFAULT_DELIVERY_TIMEOUT,
    )
    .await
    {
        Ok(results) => {
            let mut delivered = 0u32;
            let mut retrying = 0u32;
            let mut dropped = 0u32;
            let mut auto_disabled = 0u32;
            for r in &results {
                match r {
                    DispatchResult::Delivered => delivered += 1,
                    DispatchResult::Retrying => retrying += 1,
                    DispatchResult::DroppedPermanent => dropped += 1,
                    DispatchResult::DroppedAndWebhookAutoDisabled => auto_disabled += 1,
                }
            }
            if !results.is_empty() {
                tracing::info!(
                    event = "webhook.poll_swept",
                    delivered,
                    retrying,
                    dropped,
                    auto_disabled,
                    "webhook dispatch poll complete"
                );
            }
        }
        Err(err) => tracing::warn!(
            event = "webhook.poll_failed",
            error = %err,
            "webhook poll sweep failed; will retry on next tick"
        ),
    }

    // Story 31-4: drain enqueued ClickHouse ETL jobs in the same tick.
    let etl_runner = |job| run_migration(pool, job, DEFAULT_ETL_DAYS, DEFAULT_ETL_CITATION_LIMIT);
    match drain_etl_jobs(pool, DEFAULT_ETL_MAX_JOBS_PER_SWEEP, etl_runner).await {
        Ok(processed) if processed > 0 => {
            tracing::info!(event = "etl.sweep_complete", processed, "drained ETL jobs")
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(
            event = "etl.sweep_failed",
            error = %err,
            "ETL job sweep failed; will retry on next tick"
        ),
    }
}

/// Build the [`DispatchContext`] from the deployment's `opengeo.yaml` + the live
/// provider registry, mirroring the binary's boot logic. Returns `None` when the
/// config is unreadable or the registry can't be built (degraded mode: reaper +
/// webhooks + ETL still run). Shared so `ogeo serve` reuses the same wiring.
pub fn load_dispatch_context(config_path: &str) -> Option<DispatchContext> {
    let config: Option<anseo_core::Config> = std::fs::read_to_string(config_path)
        .ok()
        .and_then(|yaml| anseo_core::Config::from_yaml_str(&yaml).ok());
    let config = config?;
    let registry = match anseo_providers::registry::build_real_registry(&config) {
        Ok(reg) => reg,
        Err(err) => {
            tracing::warn!(
                event = "worker.provider_registry_unavailable",
                error = %err,
                "failed to build provider registry; scheduled ticks will not dispatch until resolved"
            );
            return None;
        }
    };
    Some(DispatchContext {
        config,
        registry,
        worker_id: format!("opengeo-worker-{}", ulid::Ulid::new()),
    })
}
