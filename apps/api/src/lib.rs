//! OpenGEO API surface — read-only endpoints powering the local Dashboard
//! (FR-17..FR-20). Phase 1 keeps the API tightly scoped to what `apps/web`
//! consumes; the public REST surface (Phase 2) builds on these handlers.

pub mod routes;

use std::str::FromStr;
use std::sync::Arc;

use axum::http::Method;
use axum::Router;
use opengeo_core::ProjectId;
use opengeo_storage::Storage;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    /// The "current project" the dashboard is reading. Phase 1 single-project
    /// deployments derive this from the `opengeo.yaml`'s brand name.
    pub project_id: ProjectId,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::runs::router())
        .merge(routes::citations::router())
        .merge(routes::visibility::router())
        .merge(routes::health::router())
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET])
                .allow_headers(Any),
        )
}

/// Parse a hex/ULID string into a `ProjectId`. Helper used by main + tests.
pub fn parse_project_id(s: &str) -> anyhow::Result<ProjectId> {
    ProjectId::from_str(s).map_err(|e| anyhow::anyhow!("invalid project id: {e}"))
}
