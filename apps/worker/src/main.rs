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
//!
//! The poll loop itself lives in [`opengeo_worker::run`] so the same
//! implementation is reused in-process by `ogeo serve` (Story 37.1). This
//! binary is the standalone entrypoint: it wires telemetry + SIGINT/SIGTERM and
//! delegates to [`opengeo_worker::run::run_poll_loop`].

use anyhow::Context;
use opengeo_core::telemetry::init_tracing;
use opengeo_scheduler::worker::REAPER_IDLE_SECONDS;
use opengeo_storage::Storage;
use opengeo_worker::run::{load_dispatch_context, run_poll_loop, POLL_INTERVAL_SECONDS};
use tokio::signal;

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

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL must be set for opengeo-worker")?;
    let storage = Storage::connect(&database_url)
        .await
        .context("failed to open postgres pool")?;
    storage
        .migrate()
        .await
        .context("failed to apply migrations")?;

    // Schedule dispatch substrate: load the base config + provider registry the
    // same way `apps/api` does so scheduled ticks drive live providers through
    // the orchestrator. Provider keys + concurrency come from `opengeo.yaml`;
    // brand identity is overlaid per project each tick by the fan-out.
    let config_path = std::env::var("OGEO_CONFIG").unwrap_or_else(|_| "opengeo.yaml".into());
    let dispatch = load_dispatch_context(&config_path);
    if dispatch.is_none() {
        tracing::warn!(
            event = "worker.dispatch_disabled",
            "no readable config or provider registry; schedule tick dispatch is inert (reaper + webhooks still run)"
        );
    }

    run_poll_loop(&storage, dispatch.as_ref(), shutdown_signal()).await
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
