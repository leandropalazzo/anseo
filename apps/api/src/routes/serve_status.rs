//! `GET /v1/serve/status` — supervisor health for `ogeo serve` (Story 37.1).
//!
//! Returns a JSON document describing the in-process supervisor state: whether
//! the API + worker are running in the same process (`ogeo serve`), when they
//! booted, and a coarse liveness indicator for each component. When the API is
//! running as a standalone binary (not via `ogeo serve`), the `supervisor`
//! field is `null` and `tier` is `"standalone"`.

use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Information injected by `ogeo serve` into the API state when running as the
/// in-process supervisor. Absent when the API runs as a standalone binary.
#[derive(Debug, Clone)]
pub struct ServeInfo {
    /// When the supervisor (and both components) booted.
    pub boot_at: DateTime<Utc>,
    /// Deployment tier name (currently always `"local"` for `ogeo serve`; future
    /// stories will add `"cloud"` and `"enterprise"` tiers).
    pub tier: String,
}

impl ServeInfo {
    /// Build a new `ServeInfo` stamped at the current UTC instant.
    pub fn new() -> Self {
        Self {
            boot_at: Utc::now(),
            tier: "local".to_string(),
        }
    }
}

/// JSON body returned by `GET /v1/serve/status`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ServeStatusResponse {
    /// `"supervisor"` when running via `ogeo serve`, `"standalone"` otherwise.
    pub mode: String,
    /// Deployment tier. `"local"` for `ogeo serve`, `"standalone"` when the API
    /// binary is used directly.
    pub tier: String,
    /// Component liveness table.
    pub components: Components,
    /// ISO-8601 boot timestamp (present when running via `ogeo serve`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_at: Option<DateTime<Utc>>,
}

/// Per-component health within the supervisor.
#[derive(Debug, Serialize, Deserialize)]
pub struct Components {
    pub api: ComponentStatus,
    pub worker: ComponentStatus,
}

/// Coarse liveness status for a single component.
#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentStatus {
    /// `"running"` while the component is alive, `"unknown"` when this endpoint
    /// is reached from a standalone (non-supervisor) deployment.
    pub status: String,
}

impl ComponentStatus {
    fn running() -> Self {
        Self {
            status: "running".to_string(),
        }
    }
    fn unknown() -> Self {
        Self {
            status: "unknown".to_string(),
        }
    }
}

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/serve/status", get(serve_status))
}

async fn serve_status(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<ServeStatusResponse> {
    match &state.serve_info {
        Some(info) => Json(ServeStatusResponse {
            mode: "supervisor".to_string(),
            tier: info.tier.clone(),
            components: Components {
                api: ComponentStatus::running(),
                // The worker is in-process alongside the API; if this handler is
                // reachable the API task is alive. The worker task joining the
                // same process lifetime means it is also considered running here.
                // Fine-grained worker liveness (heartbeat ping) is a future
                // enhancement (Story 37.x).
                worker: ComponentStatus::running(),
            },
            boot_at: Some(info.boot_at),
        }),
        None => Json(ServeStatusResponse {
            mode: "standalone".to_string(),
            tier: "standalone".to_string(),
            components: Components {
                api: ComponentStatus::running(),
                worker: ComponentStatus::unknown(),
            },
            boot_at: None,
        }),
    }
}
