//! `GET /v1/plugins` — runtime plugin load report (Story 41.2).
//!
//! Lists every installed plugin with its runtime activation status
//! (`loaded | skipped | load_error`) as computed at `anseo serve` startup by
//! [`anseo_plugin_host::loader::scan_and_load`]. The same scan powers
//! `anseo plugin list`, so the API and CLI render an identical view (AC4/AC5).
//!
//! The report is materialised once at boot and stamped into [`AppState`] so the
//! endpoint is a cheap read — it reflects the load decisions made before the
//! server accepted its first request (eager load, fail-fast). It does not
//! re-scan on every request: a freshly installed plugin requires a restart to
//! take effect, exactly as `ogeo plugin install` instructs.

use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use anseo_plugin_host::loader::LoadedPlugin;

use crate::AppState;

/// One row of `GET /v1/plugins`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatusItem {
    pub id: String,
    pub version: String,
    pub kind: String,
    /// `loaded | skipped | load_error`.
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

impl From<&LoadedPlugin> for PluginStatusItem {
    fn from(p: &LoadedPlugin) -> Self {
        PluginStatusItem {
            id: p.id.clone(),
            version: p.version.clone(),
            kind: p.kind.clone(),
            status: p.status.as_str().to_string(),
            reason: p.reason.clone(),
        }
    }
}

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/plugins", get(list_plugins))
}

async fn list_plugins(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<Vec<PluginStatusItem>> {
    let items = state
        .loaded_plugins
        .iter()
        .map(PluginStatusItem::from)
        .collect();
    Json(items)
}
