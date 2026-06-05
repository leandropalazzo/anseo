//! `X-Anseo-Project` per-request project resolution (Epic 36, ADR-004).
//!
//! Story 36.2 activates **real per-request resolution** over the `projects`
//! table and removes the boot-time single-project pin. Every `/v1/*` request
//! is resolved to a concrete [`anseo_core::ProjectId`] before any handler
//! runs; handlers read that resolved scope from request extensions (via the
//! typed [`ProjectScope`] extractor) so two requests for different projects
//! never read each other's data.
//!
//! Precedence (ADR-004), highest first:
//!
//! 1. **Explicit value** — an explicitly-supplied project identifier. At the
//!    HTTP layer the explicit value *is* the `X-Anseo-Project` header, so
//!    tiers (1) and (2) coincide here. The shared [`resolve_project`] resolver
//!    keeps the tier distinct so the CLI/MCP surfaces (later Epic 36 stories)
//!    can pass an explicit flag that out-ranks an ambient header.
//! 2. **`X-Anseo-Project` header** — resolved by brand name against the
//!    `projects` table (case-insensitive after trim, matching the
//!    `project_id_for_name` canonicalisation). The reserved sentinel
//!    `"default"` resolves to the legacy sole-active project.
//! 3. **Legacy sole-active-project fallback** — when no explicit/header value
//!    is supplied AND exactly one active project exists, resolve to it. This
//!    keeps single-project deployments working without sending the header.
//!    **Story 36.11 (RISK-6)** guarantees this tier keeps v0.2.0 single-project
//!    installations working unchanged after upgrading to the multi-project
//!    binary: no manual migration or header adoption is required.
//! 4. Otherwise -> **404 `project_not_found`**. We never silently fall back to
//!    a boot-configured default: an unknown or ambiguous project is an error,
//!    not a default.
//!
//! Enforcement is a single route-layer middleware ([`project_header_guard`])
//! mounted over the whole `/v1` surface in `apps/api/src/lib.rs`. It stamps the
//! resolved [`ProjectScope`] into request extensions; the typed extractor is a
//! noop lookup of that extension.

use anseo_core::{project_id_for_name, ProjectId as CoreProjectId};
use anseo_storage::Storage;
use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{request::Parts, HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::AppState;

/// Canonical header name. The static value is the form clients see on the wire;
/// `HeaderName::from_static` requires lowercase, so the matching helpers below
/// normalise to lowercase before lookup.
pub const PROJECT_HEADER: &str = "X-Anseo-Project";

/// Reserved sentinel resolving to "the one active project". A single-project
/// deployment can hard-code `"default"` and keep working; it resolves through
/// the legacy sole-active-project fallback.
pub const DEFAULT_PROJECT_SENTINEL: &str = "default";

/// The resolved project scope for the current request: the concrete storage
/// [`CoreProjectId`] plus the wire-visible brand name. Stamped into request
/// extensions by [`project_header_guard`]; handlers read it via the
/// [`ProjectScope`] extractor so every read is scoped to the resolved project.
#[derive(Debug, Clone)]
pub struct ProjectScope {
    /// The concrete storage id — the value every project-scoped query keys on.
    pub id: CoreProjectId,
    /// The wire-visible project (brand) name, for logging / echoing back.
    pub name: String,
}

impl ProjectScope {
    /// The resolved storage id (the isolation boundary for all reads/writes).
    pub fn id(&self) -> CoreProjectId {
        self.id
    }

    /// The wire-visible project name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Backwards-compatible alias. Earlier stories published `ProjectId` as the
/// resolved-wire-name extractor; it now carries the full resolved scope.
pub type ProjectId = ProjectScope;

/// Why resolution failed. Currently only one variant, but kept as an enum so
/// future tiers (ambiguous explicit value, archived project) map cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// No project matched the supplied precedence chain.
    NotFound,
}

/// Shared, reusable project resolver implementing the ADR-004 precedence chain.
///
/// `explicit` is a project identifier supplied out-of-band (a CLI/MCP flag);
/// `header` is the `X-Anseo-Project` value. Both are resolved by **brand
/// name** against the `projects` table. When neither is supplied (or the
/// header is the `"default"` sentinel), the legacy sole-active-project fallback
/// applies. Returns the resolved [`ProjectScope`] or [`ResolveError::NotFound`].
///
/// This is the single source of truth for project resolution so the CLI and
/// MCP surfaces (later Epic 36 stories) reuse the exact same precedence.
pub async fn resolve_project(
    storage: &Storage,
    explicit: Option<&str>,
    header: Option<&str>,
) -> Result<ProjectScope, ResolveError> {
    // Tier 1 + 2: an explicit value out-ranks the header; either is resolved
    // by brand name. The `"default"` sentinel is treated as "no value" so it
    // falls through to the legacy sole-active fallback below.
    let named = explicit
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| header.map(str::trim).filter(|s| !s.is_empty()))
        .filter(|s| !s.eq_ignore_ascii_case(DEFAULT_PROJECT_SENTINEL));

    if let Some(name) = named {
        // Derive the deterministic id from the brand name, then confirm a live
        // (non-archived) row actually exists. An id with no row is unknown.
        let id = project_id_for_name(name);
        match storage.projects().get_project(id).await {
            Ok(Some(row)) => {
                return Ok(ProjectScope {
                    id: row.id,
                    name: row.name,
                });
            }
            Ok(None) => return Err(ResolveError::NotFound),
            Err(err) => {
                tracing::error!(
                    event = "api.project_resolve.storage_error",
                    error = %err,
                    "storage error resolving named project; treating as not found"
                );
                return Err(ResolveError::NotFound);
            }
        }
    }

    // Tier 3: legacy sole-active-project fallback. Exactly one active project
    // resolves; zero or many active projects is ambiguous -> not found.
    match storage.projects().get_single_brand().await {
        Ok(Some(brand)) => Ok(ProjectScope {
            id: brand.id,
            name: brand.name,
        }),
        Ok(None) => Err(ResolveError::NotFound),
        Err(err) => {
            tracing::error!(
                event = "api.project_resolve.storage_error",
                error = %err,
                "storage error in sole-active-project fallback; treating as not found"
            );
            Err(ResolveError::NotFound)
        }
    }
}

/// Canonical project header (`X-Anseo-Project`, lowercased for `from_static`).
fn project_header_name() -> HeaderName {
    HeaderName::from_static("x-anseo-project")
}

/// Deprecated pre-rename project header, still accepted for back-compat.
fn legacy_project_header_name() -> HeaderName {
    HeaderName::from_static("x-opengeo-project")
}

/// 404 body for an unresolved project. Keeps the historical `project_not_found`
/// error shape (`error` + `error_kind`) so existing SDK consumers' error
/// handling is unchanged; only the status moves from 403 to 404 per ADR-004.
fn not_found_body() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "project_not_found",
            "error_kind": "project_not_found",
            "message": format!(
                "no project resolved for this request (checked explicit value, \
                 {PROJECT_HEADER} header, then the sole-active-project fallback)"
            ),
        })),
    )
        .into_response()
}

/// Route-layer middleware enforcing per-request resolution for every `/v1/*`
/// route. Mounted in `apps/api/src/lib.rs` alongside `require_api_key`.
pub async fn project_header_guard(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let header_name = project_header_name();
    let legacy_header = legacy_project_header_name();
    let header_value = request
        .headers()
        .get(&header_name)
        .or_else(|| request.headers().get(&legacy_header))
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    match resolve_project(&state.storage, None, header_value.as_deref()).await {
        Ok(scope) => {
            request.extensions_mut().insert(scope);
            Ok(next.run(request).await)
        }
        Err(ResolveError::NotFound) => Err(not_found_body()),
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for ProjectScope
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // The route-layer middleware (`project_header_guard`) is the sole
        // enforcement point — it runs on the entire `/v1` surface and stamps
        // the resolved `ProjectScope` into request extensions before any
        // handler sees the request. By the time this extractor fires, the
        // project is already resolved.
        parts
            .extensions
            .get::<ProjectScope>()
            .cloned()
            .ok_or_else(not_found_body)
    }
}

/// The effective project for a handler that serves BOTH the project-guarded
/// `/v1/*` surface and the legacy single-project `/api/*` dashboard surface.
///
/// On `/v1/*` the [`project_header_guard`] has already stamped a [`ProjectScope`]
/// into request extensions (real per-request resolution), so this yields it
/// verbatim. On `/api/*` — which carries no project guard and is single-project
/// by design — no scope is stamped, so this falls back to the boot-derived
/// `AppState::project_id` / `AppState::configured_project`.
///
/// Handlers that only mount under `/v1/*` can take [`ProjectScope`] directly;
/// shared handlers take `EffectiveProject` so a missing extension is the legacy
/// dashboard, not a 404.
#[derive(Debug, Clone)]
pub struct EffectiveProject {
    pub id: CoreProjectId,
    pub name: String,
}

impl EffectiveProject {
    pub fn id(&self) -> CoreProjectId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[axum::async_trait]
impl FromRequestParts<AppState> for EffectiveProject {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(scope) = parts.extensions.get::<ProjectScope>() {
            return Ok(EffectiveProject {
                id: scope.id,
                name: scope.name.clone(),
            });
        }
        // Legacy `/api/*` single-project surface: no per-request resolution.
        Ok(EffectiveProject {
            id: state.project_id,
            name: state.configured_project.as_str().to_string(),
        })
    }
}
