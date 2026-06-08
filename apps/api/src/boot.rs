//! Boot assembly for the HTTP API: connect storage, run migrations, apply the
//! DB-authoritative brand overlay, seed bootstrap key + prompts, enforce the
//! bind-acceptability guard, and build the [`AppState`] + router.
//!
//! Hoisted out of `apps/api/src/main.rs` so the exact same assembly is reused
//! **in-process** by `ogeo serve` (Story 37.1), which boots the API and the
//! worker in a single binary. The standalone `opengeo-api` binary and the
//! folded `ogeo serve` supervisor share one implementation instead of
//! duplicating ~150 lines of seeding/guard logic.

use std::sync::Arc;

use anseo_scheduler::transport::listen;
use anseo_scheduler::worker::event_channel;
use anseo_storage::Storage;
use axum::Router;

use crate::routes::serve_status::ServeInfo;
use crate::{bootstrap_key_material, check_bind_acceptable, parse_project_id, router, AppState};

/// A fully-assembled, ready-to-bind API: the validated socket address and the
/// axum router wired to its [`AppState`].
pub struct BootedApi {
    pub socket: std::net::SocketAddr,
    pub app: Router,
    /// Kept so the caller (and the NOTIFY bridge) share the same broadcast
    /// channel the router's SSE subscribers read from.
    pub events: tokio::sync::broadcast::Sender<anseo_scheduler::events::LifecycleEvent>,
}

/// Options for [`build_api`]. Mirrors the env-var reads the binary used to do
/// inline so callers (the binary, `ogeo serve`) can pass values explicitly.
pub struct ApiBootConfig {
    pub database_url: String,
    pub bind_addr: String,
    pub config_path: String,
    /// Story 37.1 — when `ogeo serve` boots the API in-process it injects
    /// supervisor metadata so `GET /v1/serve/status` can report the active
    /// tier and component liveness. `None` for standalone `opengeo-api` binary.
    pub serve_info: Option<std::sync::Arc<ServeInfo>>,
}

/// Connect storage, apply migrations, resolve the project + brand overlay, seed
/// bootstrap key/prompts, run the bind guard, and return a [`BootedApi`].
///
/// Reads the same optional env vars the binary historically honored
/// (`ANSEO_PROJECT_ID`, `ANSEO_TEST_MODE`, `ANSEO_BOOTSTRAP_API_KEY`) so
/// behavior is unchanged for the standalone binary.
pub async fn build_api(opts: ApiBootConfig) -> Result<BootedApi, Box<dyn std::error::Error>> {
    let ApiBootConfig {
        database_url,
        bind_addr,
        config_path,
        serve_info,
    } = opts;

    let loaded_config: Option<anseo_core::Config> = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|yaml| anseo_core::Config::from_yaml_str(&yaml).ok());

    let mut project_id = match std::env::var("ANSEO_PROJECT_ID") {
        Ok(s) => parse_project_id(&s)?,
        Err(_) => loaded_config
            .as_ref()
            .map(|c| c.project_id())
            .unwrap_or_default(),
    };

    let provider_registry = match loaded_config.as_ref() {
        Some(cfg) => match anseo_providers::registry::build_real_registry(cfg) {
            Ok(reg) => Some(Arc::new(reg)),
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

    let mut configured_project = loaded_config
        .as_ref()
        .map(|c| c.brand.name.clone())
        .unwrap_or_else(|| "default".to_string());
    let mut loaded_config = loaded_config;

    let storage = Arc::new(Storage::connect(&database_url).await?);
    storage.migrate().await?;

    // DB-authoritative brand config: once a project row exists, its
    // name/variants/competitors win over the bootstrap `anseo.yaml`.
    if let Some(brand) = storage.projects().get_single_brand().await? {
        project_id = anseo_core::project_id_for_name(&brand.name);
        configured_project = brand.name.clone();
        if let Some(cfg) = loaded_config.as_mut() {
            cfg.brand.name = brand.name.clone();
            cfg.brand.variants = brand.variants.clone();
            cfg.competitors = serde_json::from_value(brand.competitors).unwrap_or_default();
        }
        tracing::info!(
            event = "service.brand_db_authoritative",
            project = %project_id,
            brand = %brand.name,
            "brand config loaded from DB (overrides anseo.yaml)"
        );
    }

    let configured_project = Arc::new(configured_project);
    let loaded_config = loaded_config.map(Arc::new);

    // Story 12.1 NFR — boot-time bind acceptability check.
    let test_mode_enabled = std::env::var("ANSEO_TEST_MODE").as_deref() == Ok("1");
    let mut active_keys = storage
        .api_keys()
        .count_active_for_project(project_id)
        .await?;
    if active_keys == 0 {
        if let Ok(plaintext) = std::env::var("ANSEO_BOOTSTRAP_API_KEY") {
            let plaintext = plaintext.trim();
            if !plaintext.is_empty() {
                let (hash, prefix) =
                    bootstrap_key_material(plaintext).map_err(|msg| anyhow::anyhow!(msg))?;
                if storage.projects().get(project_id).await?.is_none() {
                    storage
                        .projects()
                        .insert(&anseo_storage::models::ProjectRow {
                            id: project_id,
                            name: (*configured_project).clone(),
                            organization_id: None,
                            tenant_id: None,
                            created_at: chrono::Utc::now(),
                        })
                        .await?;
                    tracing::info!(
                        event = "service.bootstrap_project_seeded",
                        project = %project_id,
                        "seeded projects row for bootstrap key (project did not exist)"
                    );
                }
                storage
                    .api_keys()
                    .insert(project_id, "bootstrap", &hash, &prefix)
                    .await?;
                tracing::info!(
                    event = "service.bootstrap_key_seeded",
                    project = %project_id,
                    "seeded bootstrap API key from ANSEO_BOOTSTRAP_API_KEY (project had no active keys)"
                );
                active_keys = 1;
            }
        }
    }
    // Seed declared prompts from `anseo.yaml` into the DB (DB-authoritative).
    if let Some(cfg) = loaded_config.as_ref() {
        if !cfg.prompts.is_empty()
            && storage.projects().get(project_id).await?.is_some()
            && storage
                .prompts()
                .list_by_project(project_id)
                .await?
                .is_empty()
        {
            for p in &cfg.prompts {
                let id = anseo_core::prompt_id_for(configured_project.as_str(), &p.name);
                storage
                    .prompts()
                    .insert(&anseo_storage::models::PromptRow {
                        id,
                        project_id,
                        name: p.name.clone(),
                        text: p.text.clone(),
                        tags: Vec::new(),
                        organization_id: None,
                        tenant_id: None,
                        created_at: chrono::Utc::now(),
                    })
                    .await?;
            }
            tracing::info!(
                event = "service.prompts_seeded",
                project = %project_id,
                count = cfg.prompts.len(),
                "seeded prompts from anseo.yaml (project had none)"
            );
        }
    }

    let socket = check_bind_acceptable(&bind_addr, test_mode_enabled, active_keys)
        .map_err(|msg| anyhow::anyhow!(msg))?;

    // Story 41.2 — runtime plugin activation. Eagerly scan the install
    // directory *before* the server accepts requests so every installed plugin's
    // load decision (loaded | skipped | load_error) is resolved up front.
    // Signature + platform-sandbox gates are honoured inside the loader; a
    // corrupted plugin is recorded as `load_error` and skipped — never fatal.
    let loaded_plugins = match anseo_plugin_host::loader::resolve_plugin_home() {
        Some(home) => anseo_plugin_host::loader::scan_and_load(
            &home,
            &anseo_plugin_host::loader::LoadPolicy::default(),
        ),
        None => {
            tracing::warn!(
                event = "service.plugin_home_unresolved",
                "could not resolve plugin home (no HOME/XDG_CONFIG_HOME); no plugins loaded"
            );
            Vec::new()
        }
    };
    let loaded_plugins = Arc::new(loaded_plugins);

    let (events_tx, _rx) = event_channel();
    spawn_notify_bridge(database_url.clone(), events_tx.clone());

    let state = AppState {
        storage,
        project_id,
        events: events_tx.clone(),
        config: loaded_config,
        provider_registry,
        configured_project,
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info,
        loaded_plugins,
    };
    let app = router(state);

    Ok(BootedApi {
        socket,
        app,
        events: events_tx,
    })
}

/// Bind the booted API to its validated socket and serve until `shutdown`
/// resolves, then drain in-flight requests (axum graceful shutdown).
///
/// Keeps `axum` confined to this crate so `ogeo serve` (which has no axum dep)
/// can drive the HTTP server through a plain future-based seam.
pub async fn serve_with_shutdown(
    booted: BootedApi,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind(booted.socket).await?;
    axum::serve(listener, booted.app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Bridge Postgres NOTIFY (emitted by the worker) into this process's broadcast
/// channel so SSE subscribers see worker lifecycle events. Restarts on error
/// with a 5s backoff so a transient DB blip never permanently disables SSE.
///
/// Public so `ogeo serve` reuses the exact same bridge when it runs the API and
/// worker in one process.
pub fn spawn_notify_bridge(
    database_url: String,
    events_tx: tokio::sync::broadcast::Sender<anseo_scheduler::events::LifecycleEvent>,
) {
    tokio::spawn(async move {
        loop {
            if let Err(err) = listen(&database_url, events_tx.clone()).await {
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
