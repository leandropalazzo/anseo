//! `X-OpenGEO-Project` header substrate (Story 0.11, decision L2).
//!
//! Contract — Phase 2, single configured project:
//!
//! 1. Header is **accepted** on every `/v1/*` route. SDK consumers (MCP
//!    server Epic 16, browser extension Epic 18) start sending it on day
//!    one so the Phase 4 multi-project rollout is a server-side switch.
//! 2. If the value matches the configured project name, or equals the
//!    reserved sentinel `"default"`, proceed. (Match is
//!    case-insensitive after trimming, matching the canonicalisation
//!    used by `opengeo_core::Config::project_id`.)
//! 3. If the value mismatches AND isn't `"default"`, return **403 +
//!    `error_kind: "project_not_found"`**. Phase 4 will replace this
//!    with a real lookup; for now mismatch is unambiguously a wrong
//!    project.
//! 4. If the header is absent, infer the single configured project and
//!    proceed. A **one-time WARN** per process logs the absence, so the
//!    operator sees a nudge to update SDK consumers without a per-
//!    request log storm.
//!
//! The contract is enforced two ways, by design:
//!
//! - **Route-layer middleware** ([`project_header_guard`]) wraps the
//!   entire `/v1` surface in `apps/api/src/lib.rs`. This catches every
//!   route — including the parallel Phase 3 stories (0.7-0.10, 0.12)
//!   that don't know about the header yet — without per-handler edits.
//! - **Typed extractor** ([`ProjectId`]) is published so handlers that
//!   want the resolved project name in their signature can take
//!   `_project: ProjectId`. Both layers apply the same logic; if the
//!   middleware passes, the extractor can never fail.

use std::sync::OnceLock;

use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{request::Parts, HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::AppState;

/// Canonical header name. The static value is the form clients see on
/// the wire; `HeaderName::from_static` requires lowercase, so the
/// matching helpers below normalise to lowercase before lookup.
pub const PROJECT_HEADER: &str = "X-OpenGEO-Project";

/// Reserved sentinel that always resolves to "the one configured
/// project" in Phase 2. SDKs that don't yet read project config can
/// hard-code `"default"` and continue to function under Phase 4
/// once the operator declares their project name as `"default"` or
/// updates the SDK.
pub const DEFAULT_PROJECT_SENTINEL: &str = "default";

/// One-shot guard for the "header absent" warning. The first request
/// without the header logs once at WARN; subsequent requests log
/// nothing. Per-process, not per-request, to avoid log storms.
static WARNED_ABSENT_HEADER: OnceLock<()> = OnceLock::new();

/// Resolved project name for the current request. Phase 2 carries the
/// single configured project; Phase 4 will carry the resolved scope.
#[derive(Debug, Clone)]
pub struct ProjectId(pub String);

impl ProjectId {
    /// Borrow the project name as `&str` for handlers that just want
    /// to log or thread it through to storage.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Outcome of applying the header contract.
enum HeaderDecision {
    /// Header matched the configured project, equaled `"default"`, or
    /// was absent — proceed with the resolved name.
    Accept(String),
    /// Header was present and mismatched — 403 + `project_not_found`.
    Reject,
}

fn decide(header_value: Option<&str>, configured: &str) -> HeaderDecision {
    let Some(raw) = header_value else {
        // Absence is allowed in Phase 2. Log once per process.
        if WARNED_ABSENT_HEADER.set(()).is_ok() {
            tracing::warn!(
                event = "api.project_header.absent",
                header = PROJECT_HEADER,
                "request to /v1/* without X-OpenGEO-Project header; \
                 Phase 2 single-project mode infers the configured project, \
                 but Phase 4 will require this header. Update SDK consumers."
            );
        }
        return HeaderDecision::Accept(configured.to_string());
    };
    let candidate = raw.trim();
    if candidate.eq_ignore_ascii_case(DEFAULT_PROJECT_SENTINEL)
        || candidate.eq_ignore_ascii_case(configured.trim())
    {
        return HeaderDecision::Accept(configured.to_string());
    }
    HeaderDecision::Reject
}

fn project_header_name() -> HeaderName {
    HeaderName::from_static("x-opengeo-project")
}

fn forbidden_body() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "project_not_found",
            "error_kind": "project_not_found",
            "message": format!(
                "{} header value does not match this server's configured project",
                PROJECT_HEADER
            ),
        })),
    )
        .into_response()
}

/// Route-layer middleware enforcing the contract for every `/v1/*` route.
/// Mounted in `apps/api/src/lib.rs` alongside `require_api_key`.
pub async fn project_header_guard(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let header_name = project_header_name();
    let header_value = request
        .headers()
        .get(&header_name)
        .and_then(|v| v.to_str().ok());

    let configured = state.configured_project.as_str();
    match decide(header_value, configured) {
        HeaderDecision::Accept(name) => {
            // Stamp the resolved name into request extensions so the
            // typed `ProjectId` extractor (if used) is a noop lookup.
            request.extensions_mut().insert(ProjectId(name));
            Ok(next.run(request).await)
        }
        HeaderDecision::Reject => Err(forbidden_body()),
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for ProjectId
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // The route-layer middleware (`project_header_guard`) is the sole
        // enforcement point — it runs on the entire `/v1` surface and
        // stamps the resolved `ProjectId` into request extensions before
        // any handler sees the request. So by the time this extractor
        // fires, the project is already resolved.
        //
        // Handlers that opt into the typed extractor (`_project: ProjectId`)
        // get the resolved name; handlers that don't take it are still
        // protected by the middleware.
        parts
            .extensions
            .get::<ProjectId>()
            .cloned()
            .ok_or_else(forbidden_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_value_accepted() {
        match decide(Some("acme"), "acme") {
            HeaderDecision::Accept(n) => assert_eq!(n, "acme"),
            HeaderDecision::Reject => panic!("matching value rejected"),
        }
    }

    #[test]
    fn matching_value_case_insensitive() {
        match decide(Some(" ACME "), "acme") {
            HeaderDecision::Accept(n) => assert_eq!(n, "acme"),
            HeaderDecision::Reject => panic!("case-insensitive match rejected"),
        }
    }

    #[test]
    fn default_sentinel_accepted() {
        match decide(Some("default"), "acme") {
            HeaderDecision::Accept(n) => assert_eq!(n, "acme"),
            HeaderDecision::Reject => panic!("default sentinel rejected"),
        }
    }

    #[test]
    fn mismatch_rejected() {
        assert!(matches!(decide(Some("other"), "acme"), HeaderDecision::Reject));
    }

    #[test]
    fn absent_header_accepted() {
        match decide(None, "acme") {
            HeaderDecision::Accept(n) => assert_eq!(n, "acme"),
            HeaderDecision::Reject => panic!("absent header rejected"),
        }
    }
}
