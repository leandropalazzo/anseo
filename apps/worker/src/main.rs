//! OpenGEO background worker (FR-26, ARCH-21).
//!
//! Polls due Schedule ticks, claims them at-most-once against
//! `schedule_ticks (schedule_id, tick_ts)`, publishes ARCH-17 lifecycle
//! events via Postgres NOTIFY so the API process can fan them out to SSE /
//! webhook / notification subscribers, and reaps abandoned `claimed` rows
//! after the 5-min idle window.
//!
//! Phase 2 single-host shape per architecture §6: one worker process per
//! Compose stack; multi-host distribution is Phase 4.

use anyhow::Context;
use opengeo_core::telemetry::init_tracing;
use opengeo_scheduler::events::{LifecycleEvent, SchedulePayload};
use opengeo_scheduler::transport::publish;
use opengeo_scheduler::webhooks::fanout::enqueue_lifecycle_event;
use opengeo_scheduler::webhooks::poller::{
    poll_once, DEFAULT_BATCH_LIMIT, DEFAULT_DELIVERY_TIMEOUT,
};
use opengeo_scheduler::webhooks::tick::DispatchResult;
use opengeo_scheduler::worker::{reap_orphans, REAPER_IDLE_SECONDS};
use opengeo_storage::Storage;
use std::time::Duration;
use tokio::signal;
use uuid::Uuid;

const POLL_INTERVAL_SECONDS: u64 = 5;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing("opengeo-worker")?;
    tracing::info!(
        event = "service.boot",
        service = "opengeo-worker",
        poll_interval_seconds = POLL_INTERVAL_SECONDS,
        reaper_idle_seconds = REAPER_IDLE_SECONDS,
        "starting worker"
    );

    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set for opengeo-worker")?;
    let storage = Storage::connect(&database_url)
        .await
        .context("failed to open postgres pool")?;
    storage
        .migrate()
        .await
        .context("failed to apply migrations")?;

    let pool = storage.pool().clone();
    let http_client = reqwest::Client::builder()
        .user_agent("opengeo-webhook-dispatcher/0.1")
        .build()
        .context("failed to build webhook reqwest client")?;

    let shutdown = shutdown_signal();
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
                match reap_orphans(&pool).await {
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
                            if let Err(err) = publish(&pool, &event).await {
                                tracing::warn!(
                                    event = "worker.publish_failed",
                                    error = %err,
                                    "failed to publish rolled-forward event"
                                );
                            }
                            // Fan-out to webhooks. The shim short-circuits
                            // on `schedule.tick_rolled_forward` (not a
                            // webhook-eligible kind per arch §5.3), so this
                            // is a no-op today; wired here so the bridge
                            // is in place when the worker grows to emit
                            // schedule.missed / prompt_run.completed.
                            if let Err(err) = enqueue_lifecycle_event(&storage, &event).await {
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
                // Tick discovery + claim follow-up: Story 10.4's ScheduleRepo
                // surfaces the YAML→DB sync that lets us list due rows. Until
                // then the reaper is the only active path; manual ticks
                // declared via `ogeo schedule add` still record into the
                // schedules table from Story 10.1.

                // Story 12.4: drive the webhook dispatcher in the same
                // tick. The poller fans out per-(event, webhook) tokio
                // tasks internally; failure isolation comes from that
                // per-target cardinality.
                match poll_once(&storage, &http_client, DEFAULT_BATCH_LIMIT, DEFAULT_DELIVERY_TIMEOUT).await {
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
                                delivered, retrying, dropped, auto_disabled,
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
            }
        }
    }

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl_c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
