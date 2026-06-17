//! Plugin HTTP surface — runtime load report (Story 41.2) + live marketplace
//! (Story 41.3).
//!
//! Two concerns share this module because they share the `/plugins` prefix:
//!
//!   * `GET /v1/plugins` (Story 41.2) — runtime plugin **load report**: every
//!     plugin discovered at `anseo serve` startup with its activation status
//!     (`loaded | skipped | load_error`) as computed by
//!     [`anseo_plugin_host::loader::scan_and_load`]. The same scan powers
//!     `anseo plugin list`, so API and CLI render an identical view. The report
//!     is materialised once at boot and stamped into [`AppState`], so the
//!     endpoint is a cheap read and does not re-scan per request — a freshly
//!     installed plugin requires a restart to take effect.
//!
//!   * Marketplace surface (Story 41.3) — the live GitHub flat-file registry
//!     (`anseo_plugin_host::registry`), merged with the installed-state audit
//!     table, exposed over HTTP so the dashboard `/marketplace` page and any
//!     REST client read real data (the Epic 17 mock catalog is gone):
//!       * `GET    /v1/marketplace/plugins` — registry index + installed state.
//!       * `POST   /v1/plugins/install`     — verify + record an install.
//!       * `DELETE /v1/plugins/:id`         — soft-remove an installed plugin.
//!       * `POST   /v1/plugins/:id/upgrade` — re-install the registry-current ver.
//!
//! The API server proxies the registry rather than letting the browser call
//! GitHub directly: it keeps CORS simple, allows server-side reuse of the
//! verification pipeline, and attaches no per-user credentials. The registry
//! transport is synchronous (`reqwest::blocking`, Story 41.1), so the fetches
//! run inside `spawn_blocking` to avoid stalling the async runtime.
//!
//! Signature surfacing: each marketplace plugin carries `signature_status`
//! (`signed`/`unsigned`/`revoked`) and a `verified` flag derived from whether
//! the registry artifact carries a root-signed namespace claim. The UI renders
//! the `[UNSIGNED PLUGIN]` / verified badge from these fields.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use anseo_plugin_host::loader::LoadedPlugin;
use anseo_plugin_host::registry::{IndexEntry, RegistryClient, RegistryError};
use anseo_plugin_host::signing::{pinned_root_pubkeys, SignatureStatus};
use anseo_plugin_manifest::PluginManifest;
use anseo_storage::repositories::plugin_installs::{
    NewPluginInstall, PluginInstallRow, PluginInstallsRepo,
};

use crate::AppState;

type ApiError = (StatusCode, Json<JsonValue>);

fn err(status: StatusCode, error: &str, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(json!({ "error": error, "message": message.into() })),
    )
}

pub fn v1_router() -> Router<AppState> {
    Router::new()
        // Story 41.2 — runtime load report (loaded plugins at `anseo serve`).
        .route("/plugins", get(list_plugins))
        // Story 41.3 — live marketplace + install lifecycle.
        .route("/marketplace/plugins", get(marketplace_handler))
        .route("/plugins/install", post(install_handler))
        .route("/plugins/:id", delete(remove_handler))
        .route("/plugins/:id/upgrade", post(upgrade_handler))
}

// ---------------------------------------------------------------------------
// GET /v1/plugins — runtime load report (Story 41.2).
// ---------------------------------------------------------------------------

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

async fn list_plugins(State(state): State<AppState>) -> Json<Vec<PluginStatusItem>> {
    let items = state
        .loaded_plugins
        .iter()
        .map(PluginStatusItem::from)
        .collect();
    Json(items)
}

// ---------------------------------------------------------------------------
// Wire shape — mirrors apps/web/lib/api/marketplace.ts `MarketplacePlugin`.
// ---------------------------------------------------------------------------

/// One plugin as the dashboard `/marketplace` page consumes it.
#[derive(Debug, Clone, Serialize)]
pub struct MarketplacePlugin {
    /// `namespace/name` — the detail-route slug and the install id.
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub homepage: String,
    pub plugin_type: String,
    /// Verified publisher (root-signed namespace claim present).
    pub verified: bool,
    /// `signed` | `unsigned` | `revoked`.
    pub signature_status: String,
    pub capabilities: JsonValue,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    pub update_available: bool,
}

#[derive(Debug, Serialize)]
struct MarketplaceResponse {
    plugins: Vec<MarketplacePlugin>,
}

#[derive(Debug, Clone)]
struct MarketplaceManifestMetadata {
    name: String,
    author: String,
    homepage: String,
    plugin_type: String,
    capabilities: JsonValue,
}

// ---------------------------------------------------------------------------
// GET /v1/marketplace/plugins
// ---------------------------------------------------------------------------

async fn marketplace_handler(
    State(state): State<AppState>,
) -> Result<Json<MarketplaceResponse>, ApiError> {
    // Installed state first (cheap, local). Keyed by plugin id (== slug).
    let installed = PluginInstallsRepo::new(state.storage.pool())
        .find_active()
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string()))?;
    let installed_by_id: HashMap<String, PluginInstallRow> = installed
        .into_iter()
        .map(|r| (r.plugin_name.clone(), r))
        .collect();

    // Registry index over the blocking HTTP transport. A transport failure
    // (registry offline) degrades to an empty list so the dashboard can render
    // its zero-state (AC4) rather than erroring.
    let entries: Vec<IndexEntry> = tokio::task::spawn_blocking(|| {
        let client = RegistryClient::from_env();
        client.search_lenient("")
    })
    .await
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "join_error",
            e.to_string(),
        )
    })?
    .unwrap_or_default();

    // Collapse to the registry-current (highest) version per id; that is the
    // single version the registry advertises as installable (Story scope:
    // "no semver resolution — registry declares exactly one current version").
    let mut current: HashMap<String, IndexEntry> = HashMap::new();
    for e in entries {
        current
            .entry(e.id.clone())
            .and_modify(|cur| {
                if e.version > cur.version {
                    *cur = e.clone();
                }
            })
            .or_insert(e);
    }

    let installed_for_lookup = installed_by_id.clone();
    let mut plugins: Vec<MarketplacePlugin> = tokio::task::spawn_blocking(move || {
        let client = RegistryClient::from_env();
        current
            .into_values()
            .map(|entry| {
                let metadata = client
                    .fetch_manifest(&entry.id, &entry.version)
                    .ok()
                    .map(|fetched| metadata_from_manifest(&fetched.manifest));
                let inst = installed_for_lookup.get(&entry.id);
                into_marketplace_plugin(entry, metadata, inst)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "join_error",
            e.to_string(),
        )
    })?;

    // Installed plugins that are no longer in the registry index still belong
    // on the Installed tab, so surface them too.
    for (id, row) in &installed_by_id {
        if !plugins.iter().any(|p| &p.slug == id) {
            plugins.push(installed_only_plugin(row));
        }
    }

    plugins.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(Json(MarketplaceResponse { plugins }))
}

/// Build the marketplace row for a registry index entry, merging install state.
fn into_marketplace_plugin(
    e: IndexEntry,
    metadata: Option<MarketplaceManifestMetadata>,
    inst: Option<&PluginInstallRow>,
) -> MarketplacePlugin {
    let slug = e.id.clone();
    let manifest_capabilities = metadata
        .as_ref()
        .map(|meta| meta.capabilities.clone())
        .unwrap_or_else(|| json!([]));
    let (installed, installed_version, signature_status, verified, capabilities) = match inst {
        Some(row) => (
            true,
            Some(row.plugin_version.clone()),
            install_signature_status(row),
            row.signature_verified,
            row.capability_set.clone(),
        ),
        // Not installed: the registry index does not carry signature material,
        // so report an indeterminate "unsigned/unverified" trust state until
        // the install pipeline verifies it (the install path is what proves a
        // signature). Capabilities are surfaced at install time / detail fetch.
        None => (
            false,
            None,
            "unsigned".to_string(),
            false,
            manifest_capabilities,
        ),
    };
    let plugin_type = metadata
        .as_ref()
        .map(|meta| meta.plugin_type.clone())
        .unwrap_or_else(|| "extractor".to_string());
    let (name, author, homepage) = metadata
        .map(|meta| (meta.name, meta.author, meta.homepage))
        .unwrap_or_else(|| (slug.clone(), String::new(), String::new()));
    // `latest` (registry-current) differs from the installed version ⇒ upgrade.
    let update_available = installed_version.as_deref().is_some_and(|v| v != e.version);
    MarketplacePlugin {
        name,
        slug,
        version: e.version,
        description: e.description,
        author,
        homepage,
        plugin_type,
        verified,
        signature_status,
        capabilities,
        installed,
        installed_version,
        update_available,
    }
}

fn metadata_from_manifest(manifest: &PluginManifest) -> MarketplaceManifestMetadata {
    MarketplaceManifestMetadata {
        name: manifest.name.clone(),
        author: manifest.author.clone(),
        homepage: manifest.homepage.clone(),
        plugin_type: manifest.plugin_type.to_string(),
        capabilities: serde_json::to_value(&manifest.capabilities).unwrap_or_else(|_| json!([])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_plugin_manifest::{Capability, PluginType};

    fn sample_entry() -> IndexEntry {
        IndexEntry {
            id: "acme/warehouse".to_string(),
            version: "1.2.3".to_string(),
            description: "Warehouse connector".to_string(),
            sha256: "00".repeat(32),
            yanked: false,
        }
    }

    #[test]
    fn marketplace_rows_use_manifest_metadata_for_registry_plugins() {
        let plugin = into_marketplace_plugin(
            sample_entry(),
            Some(MarketplaceManifestMetadata {
                name: "Warehouse Sync".to_string(),
                author: "Acme".to_string(),
                homepage: "https://example.com/plugins/warehouse".to_string(),
                plugin_type: "provider".to_string(),
                capabilities: json!([
                    { "kind": "network", "allowlist": ["warehouse.example.com"] }
                ]),
            }),
            None,
        );

        assert_eq!(plugin.name, "Warehouse Sync");
        assert_eq!(plugin.author, "Acme");
        assert_eq!(plugin.homepage, "https://example.com/plugins/warehouse");
        assert_eq!(plugin.plugin_type, "provider");
        assert_eq!(
            plugin.capabilities,
            json!([{ "kind": "network", "allowlist": ["warehouse.example.com"] }])
        );
    }

    #[test]
    fn manifest_metadata_serializes_plugin_shape_for_ui() {
        let manifest = PluginManifest {
            name: "acme/warehouse".to_string(),
            version: "1.2.3".to_string(),
            description: "Warehouse connector".to_string(),
            author: "Acme".to_string(),
            publisher: String::new(),
            homepage: "https://example.com/plugins/warehouse".to_string(),
            capabilities: vec![Capability::Network {
                allowlist: vec!["warehouse.example.com".to_string()],
            }],
            plugin_type: PluginType::Provider,
            entry_point: "entrypoint.wasm".into(),
        };

        let metadata = metadata_from_manifest(&manifest);

        assert_eq!(metadata.name, "acme/warehouse");
        assert_eq!(metadata.author, "Acme");
        assert_eq!(metadata.homepage, "https://example.com/plugins/warehouse");
        assert_eq!(metadata.plugin_type, "provider");
        assert_eq!(
            metadata.capabilities,
            json!([{ "kind": "network", "allowlist": ["warehouse.example.com"] }])
        );
    }
}

/// An installed plugin absent from the current registry index (e.g. yanked or
/// a private install). Render it on the Installed tab from the audit row alone.
fn installed_only_plugin(row: &PluginInstallRow) -> MarketplacePlugin {
    MarketplacePlugin {
        name: row.plugin_name.clone(),
        slug: row.plugin_name.clone(),
        version: row.plugin_version.clone(),
        description: String::new(),
        author: String::new(),
        homepage: String::new(),
        plugin_type: "extractor".to_string(),
        verified: row.signature_verified,
        signature_status: install_signature_status(row),
        capabilities: row.capability_set.clone(),
        installed: true,
        installed_version: Some(row.plugin_version.clone()),
        update_available: false,
    }
}

/// Map an audit row's verification fields to the UI signature status.
fn install_signature_status(row: &PluginInstallRow) -> String {
    if row.signature_verified {
        "signed".to_string()
    } else {
        "unsigned".to_string()
    }
}

// ---------------------------------------------------------------------------
// POST /v1/plugins/install
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct InstallBody {
    /// `namespace/name` (registry id). The registry-current version is used.
    id: String,
    /// Operator acknowledged installing an unsigned plugin (UX-DR101).
    #[serde(default)]
    acknowledge_unsigned: bool,
}

#[derive(Debug, Serialize)]
struct InstallResult {
    ok: bool,
    signature_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    audit_event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_kind: Option<String>,
    message: String,
}

async fn install_handler(
    State(state): State<AppState>,
    Json(body): Json<InstallBody>,
) -> Result<Json<InstallResult>, ApiError> {
    do_install(&state, &body.id, "latest", body.acknowledge_unsigned).await
}

async fn upgrade_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<InstallResult>, ApiError> {
    // Upgrade re-installs whatever the registry advertises as current. Signed
    // upgrades verify as usual; an unsigned current artifact requires the same
    // acknowledgment, which the UI re-prompts for on a failed upgrade.
    do_install(&state, &id, "latest", false).await
}

/// Verify a registry artifact and record the install. The on-disk materialize
/// and worker hot-load belong to `ogeo plugin install` and Story 41.2's
/// load-path; this HTTP surface performs the verification plus audit-record
/// half so the dashboard install button has a real, signature-checked backend.
async fn do_install(
    state: &AppState,
    id: &str,
    version: &str,
    acknowledge_unsigned: bool,
) -> Result<Json<InstallResult>, ApiError> {
    let id_owned = id.to_string();
    let version_owned = version.to_string();

    // Fetch + verify on the blocking transport.
    let verified = tokio::task::spawn_blocking(move || {
        let client = RegistryClient::from_env();
        let roots = pinned_root_pubkeys();
        client.fetch_verified(&id_owned, &version_owned, &roots, None)
    })
    .await
    .map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "join_error",
            e.to_string(),
        )
    })?;

    let verified = match verified {
        Ok(v) => v,
        Err(RegistryError::Unsigned {
            version: resolved_version,
            ..
        }) => {
            // Unsigned artifact: only proceed when the operator acknowledged.
            if !acknowledge_unsigned {
                return Ok(Json(InstallResult {
                    ok: false,
                    signature_status: "unsigned".to_string(),
                    audit_event_id: None,
                    error_kind: Some("signing_failed".to_string()),
                    message: "Unsigned plugin — install not acknowledged.".to_string(),
                }));
            }
            // Acknowledged unsigned: record the install without signature proof.
            // `resolved_version` is the concrete registry-current version the
            // resolver picked for `latest`, so the audit row records exactly
            // what was installed rather than the literal string "latest".
            return record_unsigned_install(state, id, &resolved_version).await;
        }
        Err(RegistryError::Verification { source, .. }) => {
            return Ok(Json(InstallResult {
                ok: false,
                signature_status: "revoked".to_string(),
                audit_event_id: None,
                error_kind: Some("signing_failed".to_string()),
                message: format!("Signature verification failed: {source}"),
            }));
        }
        Err(RegistryError::UnknownPlugin { id, version }) => {
            return Err(err(
                StatusCode::NOT_FOUND,
                "unknown_plugin",
                format!("`{id}@{version}` is not in the registry index"),
            ));
        }
        Err(RegistryError::Transport { .. } | RegistryError::NotFound { .. }) => {
            return Ok(Json(InstallResult {
                ok: false,
                signature_status: "unsigned".to_string(),
                audit_event_id: None,
                error_kind: Some("network".to_string()),
                message: "Couldn't reach the registry to complete the install.".to_string(),
            }));
        }
        Err(e) => {
            return Err(err(
                StatusCode::BAD_GATEWAY,
                "registry_error",
                e.to_string(),
            ))
        }
    };

    // Record the verified install in the audit table.
    let capability_set =
        serde_json::to_value(&verified.manifest.capabilities).unwrap_or_else(|_| json!([]));
    let signed = matches!(verified.status, SignatureStatus::Signed);
    let actor = "dashboard";
    let audit_id = PluginInstallsRepo::new(state.storage.pool())
        .insert(NewPluginInstall {
            plugin_name: &verified.id,
            plugin_version: &verified.version,
            publisher_pubkey_fingerprint: &hex::encode(verified.author_key_to_pin),
            installed_by_actor: actor,
            capability_set,
            signature_verified: signed,
            signing_trust_root: "first-party-root",
        })
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string()))?;

    Ok(Json(InstallResult {
        ok: true,
        signature_status: if signed { "signed" } else { "unsigned" }.to_string(),
        audit_event_id: Some(audit_id.to_string()),
        error_kind: None,
        message: "Plugin installed. Restart the worker to load it.".to_string(),
    }))
}

async fn record_unsigned_install(
    state: &AppState,
    id: &str,
    version: &str,
) -> Result<Json<InstallResult>, ApiError> {
    let audit_id = PluginInstallsRepo::new(state.storage.pool())
        .insert(NewPluginInstall {
            plugin_name: id,
            plugin_version: version,
            publisher_pubkey_fingerprint: "unsigned",
            installed_by_actor: "dashboard",
            capability_set: json!([]),
            signature_verified: false,
            signing_trust_root: "unsigned",
        })
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string()))?;
    Ok(Json(InstallResult {
        ok: true,
        signature_status: "unsigned".to_string(),
        audit_event_id: Some(audit_id.to_string()),
        error_kind: None,
        message: "Unsigned plugin installed. Restart the worker to load it.".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// DELETE /v1/plugins/:id
// ---------------------------------------------------------------------------

async fn remove_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let removed = PluginInstallsRepo::new(state.storage.pool())
        .mark_removed(&id, Some("dashboard remove"))
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "db_error", e.to_string()))?;
    if removed == 0 {
        return Err(err(
            StatusCode::NOT_FOUND,
            "not_installed",
            format!("no active install found for `{id}`"),
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}
