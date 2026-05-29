//! `opengeo-api` — Axum HTTP service that powers the local Dashboard.

use std::sync::Arc;

use opengeo_api::{check_bind_acceptable, parse_project_id, router, AppState};
use opengeo_core::telemetry::init_tracing;
use opengeo_scheduler::transport::listen;
use opengeo_scheduler::worker::event_channel;
use opengeo_storage::Storage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("opengeo-api")?;

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    // Spec env var: `OPENGEO_API_BIND` (Story 12.1 A-13). The legacy
    // `OGEO_BIND_ADDR` is honored as a fallback so existing Compose
    // configs keep booting until they migrate.
    let bind_addr = std::env::var("OPENGEO_API_BIND")
        .or_else(|_| std::env::var("OGEO_BIND_ADDR"))
        .unwrap_or_else(|_| "127.0.0.1:8080".into());
    // Load project config + provider registry up front so the API-mode
    // entry point in `routes::prompt_runs` can drive live providers via
    // the same `build_real_registry` the CLI uses. Failing to read the
    // YAML is non-fatal — the read-only surface still works, and the
    // write surface returns a clear 503 when config is absent.
    let config_path = std::env::var("OGEO_CONFIG").unwrap_or_else(|_| "opengeo.yaml".into());
    let loaded_config: Option<opengeo_core::Config> = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|yaml| opengeo_core::Config::from_yaml_str(&yaml).ok());

    let project_id = match std::env::var("OGEO_PROJECT_ID") {
        Ok(s) => parse_project_id(&s)?,
        Err(_) => loaded_config
            .as_ref()
            .map(|c| c.project_id())
            .unwrap_or_else(opengeo_core::ProjectId::new),
    };

    let provider_registry = match loaded_config.as_ref() {
        Some(cfg) => match opengeo_providers::registry::build_real_registry(cfg) {
            Ok(reg) => Some(std::sync::Arc::new(reg)),
            Err(err) => {
                tracing::warn!(
                    event = "service.provider_registry_unavailable",
                    error = %err,
                    "failed to build provider registry; live POST /v1/prompt-runs will 503 until resolved"
                );
                None
            }
        },
        None => None,
    };
    let loaded_config = loaded_config.map(std::sync::Arc::new);

    let storage = Arc::new(Storage::connect(&database_url).await?);
    storage.migrate().await?;

    // Story 12.1 NFR — boot-time bind acceptability check.
    // Logic extracted into `opengeo_api::check_bind_acceptable` so it's
    // unit-testable; here we wire the env-var read + DB count.
    let test_mode_enabled =
        std::env::var("OPENGEO_TEST_MODE").as_deref() == Ok("1");
    let active_keys = storage
        .api_keys()
        .count_active_for_project(project_id)
        .await?;
    let socket_preview = check_bind_acceptable(&bind_addr, test_mode_enabled, active_keys)
        .map_err(|msg| anyhow::anyhow!(msg))?;

    let (events_tx, _rx) = event_channel();
    {
        // Bridge Postgres NOTIFY (from the worker process) into this
        // process's broadcast channel so SSE subscribers see worker events.
        // Restarting on error: a broken listener is recoverable; we never
        // want a transient DB blip to silently disable the SSE pipeline.
        let url = database_url.clone();
        let tx = events_tx.clone();
        tokio::spawn(async move {
            // `listen` is an infinite loop internally; it only returns on
            // a fatal connection / channel error. The respawn here catches
            // those errors and waits 5 s before reconnecting so a flapping
            // database doesn't pin the CPU.
            loop {
                if let Err(err) = listen(&url, tx.clone()).await {
                    tracing::warn!(
                        event = "transport.listener_failed",
                        error = %err,
                        "lifecycle listener crashed; restarting in 5s"
                    );
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    }
    let state = AppState {
        storage,
        project_id,
        events: events_tx,
        config: loaded_config,
        provider_registry,
    };
    let app = router(state);

    // Reuse the already-validated socket address from the boot-time check.
    tracing::info!(event = "service.boot", service = "opengeo-api", bind = %socket_preview);
    let listener = tokio::net::TcpListener::bind(socket_preview).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
