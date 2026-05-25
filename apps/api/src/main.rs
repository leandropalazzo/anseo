//! `opengeo-api` — Axum HTTP service that powers the local Dashboard.

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use opengeo_api::{parse_project_id, router, AppState};
use opengeo_core::telemetry::init_tracing;
use opengeo_storage::Storage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing("opengeo-api")?;

    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;
    let bind_addr = std::env::var("OGEO_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let project_id = match std::env::var("OGEO_PROJECT_ID") {
        Ok(s) => parse_project_id(&s)?,
        Err(_) => {
            // Fall back to the brand-name-derived project_id from opengeo.yaml
            // if present; otherwise generate a fresh one (single-project demo).
            let path = std::env::var("OGEO_CONFIG").unwrap_or_else(|_| "opengeo.yaml".into());
            if let Ok(yaml) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = opengeo_core::Config::from_yaml_str(&yaml) {
                    cfg.project_id()
                } else {
                    opengeo_core::ProjectId::new()
                }
            } else {
                opengeo_core::ProjectId::new()
            }
        }
    };

    let storage = Arc::new(Storage::connect(&database_url).await?);
    storage.migrate().await?;

    let state = AppState {
        storage,
        project_id,
    };
    let app = router(state);

    let socket = SocketAddr::from_str(&bind_addr)
        .map_err(|e| anyhow::anyhow!("invalid OGEO_BIND_ADDR `{bind_addr}`: {e}"))?;
    tracing::info!(event = "service.boot", service = "opengeo-api", bind = %socket);
    let listener = tokio::net::TcpListener::bind(socket).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
