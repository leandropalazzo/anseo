//! `opengeo-api` — Axum HTTP service that powers the local Dashboard.
//!
//! Boot assembly (storage connect, brand overlay, seeding, bind guard, router)
//! lives in [`anseo_api::boot`] so the same wiring is reused in-process by
//! `ogeo serve` (Story 37.1). This binary is a thin wrapper: read env vars,
//! build, bind, serve.

use anseo_api::boot::{build_api, ApiBootConfig};
use anseo_core::telemetry::init_tracing;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("anseo-api")?;

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    // Spec env var: `ANSEO_API_BIND` (Story 12.1 A-13). The legacy vars
    // `OPENGEO_API_BIND`, `ANSEO_BIND_ADDR`, and `OGEO_BIND_ADDR` are honored
    // as fallbacks so existing Compose configs keep booting until they migrate.
    let bind_addr = std::env::var("ANSEO_API_BIND")
        .or_else(|_| std::env::var("OPENGEO_API_BIND")) // deprecated
        .or_else(|_| std::env::var("ANSEO_BIND_ADDR"))
        .or_else(|_| std::env::var("OGEO_BIND_ADDR")) // deprecated
        .unwrap_or_else(|_| "127.0.0.1:8080".into());
    // Config file: ANSEO_CONFIG (default anseo.yaml). Also accept OPENGEO_CONFIG
    // and OGEO_CONFIG for one-release back-compat.
    let config_path = std::env::var("ANSEO_CONFIG")
        .or_else(|_| std::env::var("OPENGEO_CONFIG")) // deprecated
        .or_else(|_| std::env::var("OGEO_CONFIG")) // deprecated
        .unwrap_or_else(|_| "anseo.yaml".into());

    let booted = build_api(ApiBootConfig {
        database_url,
        bind_addr,
        config_path,
        serve_info: None,
    })
    .await?;

    tracing::info!(event = "service.boot", service = "anseo-api", bind = %booted.socket);
    let listener = tokio::net::TcpListener::bind(booted.socket).await?;
    // `into_make_service_with_connect_info` so the public site-events ingest
    // (Story 47.1) can read the peer socket IP for its in-memory rate limiter
    // when no `X-Forwarded-For` is present (local dev). The IP is never stored.
    axum::serve(
        listener,
        booted
            .app
            .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
