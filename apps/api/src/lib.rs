//! OpenGEO API surface — read-only endpoints powering the local Dashboard
//! (FR-17..FR-20). Phase 1 keeps the API tightly scoped to what `apps/web`
//! consumes; the public REST surface (Phase 2) builds on these handlers.

pub mod extractors;
pub mod middleware;
pub mod routes;
pub mod setup_probe;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use axum::http::Method;
use axum::Router;
use opengeo_core::{Config, ProjectId};
use opengeo_providers::ProviderRegistry;
use opengeo_scheduler::events::LifecycleEvent;
use opengeo_storage::Storage;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};
use ulid::Ulid;

use crate::routes::setup::InstallState;

use crate::middleware::auth::require_api_key;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<Storage>,
    /// The "current project" the dashboard is reading. Phase 1 single-project
    /// deployments derive this from the `opengeo.yaml`'s brand name.
    pub project_id: ProjectId,
    /// Phase 2 ARCH-16 broadcast — the worker, webhook dispatcher, and
    /// notification channels each publish here; the SSE route subscribes
    /// per-client.
    pub events: broadcast::Sender<LifecycleEvent>,
    /// Project `Config` loaded at boot from `opengeo.yaml`. Carries the
    /// prompt + provider declarations needed by the orchestrator's API-
    /// mode entry point in `routes::prompt_runs`. Optional so dev binds
    /// (where the YAML may be absent) still boot for the read-only
    /// surface; the write path returns a clear 503 if missing.
    pub config: Option<Arc<Config>>,
    /// Live `ProviderRegistry` constructed from configured secrets via
    /// `opengeo_providers::registry::build_real_registry`. Providers
    /// without a secret are absent here; the orchestrator synthesises a
    /// `failed` record for them on dispatch.
    pub provider_registry: Option<Arc<ProviderRegistry>>,
    /// Story 0.11 substrate — the single configured project's wire name
    /// (the `brand.name` from `opengeo.yaml`). The `X-OpenGEO-Project`
    /// header is matched (case-insensitive after trim) against this
    /// value. Phase 4 multi-project will replace this with a resolver
    /// over the projects table.
    pub configured_project: Arc<String>,
    /// Story 15.1 — in-memory progress map for `POST /v1/setup/clickhouse/install`.
    /// Keyed by the install `ulid` returned in the 202 response; populated by
    /// the background mock state machine in `routes::setup`. Lives in
    /// `AppState` so the SSE stream handler can find the same install across
    /// requests. The map is intentionally process-local — the install flow
    /// is operator-driven, not multi-instance.
    pub setup_install_state: Arc<RwLock<HashMap<Ulid, InstallState>>>,
}

pub fn router(state: AppState) -> Router {
    // Two coexisting surfaces, both gated by `require_api_key` when active
    // keys exist (operators can still run un-keyed against `127.0.0.1` for
    // dev; the boot-time bind refusal at `apps/api/src/main` covers the
    // public-interface case).
    //
    // - **Root paths** `/api/runs`, `/api/citations`, `/api/visibility`,
    //   `/healthz`. The Phase 1 dashboard at `apps/web` consumes these.
    //   Now auth-gated (Decision 3 of the Story 12.1 review): the
    //   dashboard will need to forward an `X-OpenGEO-API-Key` header.
    // - **`/v1/*`** — Phase 2 public REST surface. Same handlers,
    //   path-stripped of the legacy `/api/` segment, plus the SSE events
    //   route at `/v1/projects/:project_id/events`. Same auth gate.
    //
    // Phase 4 multi-project will re-scope the reads under
    // `/v1/projects/:project_id/runs` etc — Phase 2 is single-project so
    // the flat shape works. The deviation from architecture-phase2.md
    // §4.1's project-scoped surface is codified as the
    // `AD-Phase2-PathScoping` architecture-delta entry in the same doc
    // (immediately after the §4.1 tables); see that entry for rationale,
    // SDK-consumer impact, and the forward-path / deprecation plan.
    let phase_1_reads_at_root = Router::new()
        .merge(routes::runs::router())
        .merge(routes::citations::router())
        .merge(routes::visibility::router())
        .merge(routes::health::router());

    let phase_1_reads_under_v1 = Router::new()
        .merge(routes::runs::v1_router())
        .merge(routes::citations::v1_router())
        .merge(routes::visibility::v1_router())
        .merge(routes::health::v1_router());

    let v1_surface = Router::new()
        .merge(phase_1_reads_under_v1)
        .merge(routes::prompt_runs::v1_router())
        .merge(routes::prompts::v1_router())
        .merge(routes::brands::v1_router())
        .merge(routes::analytics::v1_router())
        .merge(routes::anomalies::v1_router())
        .merge(routes::comparisons::v1_router())
        .merge(routes::schedules::v1_router())
        .merge(routes::setup::v1_router())
        .merge(routes::prompts_similarity::v1_router())
        .merge(routes::events::router_under_v1_relative())
        // Story 0.11 — X-OpenGEO-Project header substrate. Layered
        // INSIDE the auth gate so unauthenticated callers still get a
        // 401 before any project-header consideration. Each layer is
        // applied bottom-up by Axum, so this guard runs AFTER
        // `require_api_key` on each request.
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            extractors::project_header_guard,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));

    let phase_1_at_root_gated = phase_1_reads_at_root.route_layer(
        axum::middleware::from_fn_with_state(state.clone(), require_api_key),
    );

    let mut base = Router::new()
        .merge(phase_1_at_root_gated)
        .nest("/v1", v1_surface);

    // `POST /test/seed` is registered only when OPENGEO_TEST_MODE=1. The
    // env-var gate lives at router build time so production binaries never
    // expose the route, regardless of which HTTP layer middleware applies.
    if routes::test_seed::is_enabled_via_env() {
        tracing::warn!(
            event = "service.test_mode_enabled",
            "OPENGEO_TEST_MODE=1 detected — mounting POST /test/seed. This MUST NOT be set in production."
        );
        base = base.merge(routes::test_seed::router());
    }

    base.with_state(state).layer(
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers(Any),
    )
}

/// Parse a hex/ULID string into a `ProjectId`. Helper used by main + tests.
pub fn parse_project_id(s: &str) -> anyhow::Result<ProjectId> {
    ProjectId::from_str(s).map_err(|e| anyhow::anyhow!("invalid project id: {e}"))
}

/// Pure boot-time bind-refusal logic (Story 12.1 NFR-AuthHardenedByDefault).
/// Returns `Ok(())` if the bind is acceptable, `Err(message)` otherwise.
/// Extracted from `apps/api/src/main.rs` so the policy is unit-testable
/// without standing up a server.
///
/// Rules:
/// 1. Bind address must parse as a `SocketAddr` (IP literal + port).
/// 2. If the address is a loopback IP, accept (no auth gate needed).
/// 3. If non-loopback AND `OPENGEO_TEST_MODE=1`, refuse — `/test/seed` is
///    unauthenticated by design and must never reach the open internet.
/// 4. If non-loopback AND zero active keys exist for this project, refuse —
///    a public bind with no keys is unauthenticated by definition.
pub fn check_bind_acceptable(
    bind_addr: &str,
    test_mode_enabled: bool,
    active_keys_for_project: i64,
) -> Result<std::net::SocketAddr, String> {
    let socket = std::net::SocketAddr::from_str(bind_addr)
        .map_err(|e| format!("invalid OPENGEO_API_BIND `{bind_addr}`: {e}"))?;
    if socket.ip().is_loopback() {
        return Ok(socket);
    }
    if test_mode_enabled {
        return Err(format!(
            "OPENGEO_API_BIND=`{bind_addr}` is non-loopback AND OPENGEO_TEST_MODE=1 — \
             refusing to start. The /test/seed surface is unauthenticated by design; \
             it must never be reachable on a public interface."
        ));
    }
    if active_keys_for_project == 0 {
        return Err(format!(
            "OPENGEO_API_BIND=`{bind_addr}` is non-loopback but no active API keys exist \
             for this project. Generate one with `ogeo api key create --name <slug>` \
             before binding to a public interface, or bind to 127.0.0.1 / ::1 for \
             local-only access."
        ));
    }
    Ok(socket)
}
