//! `opengeo-api` — Axum HTTP service that powers the local Dashboard.
//!
//! Boot assembly (storage connect, brand overlay, seeding, bind guard, router)
//! lives in [`opengeo_api::boot`] so the same wiring is reused in-process by
//! `ogeo serve` (Story 37.1). This binary is a thin wrapper: read env vars,
//! build, bind, serve.

use opengeo_api::boot::{build_api, ApiBootConfig};
use opengeo_core::telemetry::init_tracing;

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
    let config_path = std::env::var("OGEO_CONFIG").unwrap_or_else(|_| "opengeo.yaml".into());

    let booted = build_api(ApiBootConfig {
        database_url,
        bind_addr,
        config_path,
        serve_info: None,
    })
    .await?;

    tracing::info!(event = "service.boot", service = "opengeo-api", bind = %booted.socket);
    let listener = tokio::net::TcpListener::bind(booted.socket).await?;
    axum::serve(listener, booted.app).await?;
    Ok(())
}
