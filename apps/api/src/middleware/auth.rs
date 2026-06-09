//! API-key auth middleware for Story 12.1.
//!
//! The hot path:
//!   1. Read the `X-OpenGEO-API-Key` header (spec A-13).
//!   2. `extract_api_key` returns the raw token (no scheme prefix).
//!   3. `looks_like_key` rejects malformed shapes without a DB round-trip
//!      (the cheapest filter; protects the lookup index from junk).
//!   4. `sha256_hex` over the token, then `ApiKeyRepo::lookup_active_project`.
//!   5. On hit: stamp `AuthenticatedProject(project_id)` into request
//!      extensions and call `next`. On miss: 401.
//!   6. After the handler returns, `touch_last_used` runs as a fire-and-
//!      forget update — operators see "last used" within one request of
//!      reality without paying the latency cost on the response path.
//!
//! Pure-logic verification lives in `anseo_core::api_key`; this module
//! wires it to axum.

use anseo_core::api_key::{extract_api_key, looks_like_key, sha256_hex, API_KEY_HEADER};
use anseo_core::ProjectId;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::AppState;

/// Marker for a successfully-authenticated request. Inserted into the
/// `Request` extensions so route handlers can borrow the authorized
/// project scope.
#[derive(Debug, Clone, Copy)]
pub struct AuthenticatedProject(pub ProjectId);

/// `axum::middleware::from_fn_with_state` entry point.
pub async fn require_api_key(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Read the canonical `X-Anseo-API-Key` header, falling back to the
    // deprecated `X-OpenGEO-API-Key` for back-compat with pre-rename clients.
    // HeaderName::from_static caches statically and requires lowercase.
    let anseo_header: HeaderName = HeaderName::from_static("x-anseo-api-key");
    let legacy_header: HeaderName = HeaderName::from_static("x-opengeo-api-key");
    let header_value = request
        .headers()
        .get(&anseo_header)
        .or_else(|| request.headers().get(&legacy_header))
        .and_then(|v| v.to_str().ok());

    let Some(token) = extract_api_key(header_value) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if !looks_like_key(token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let _ = API_KEY_HEADER; // wire-shape constant; reference keeps the import live.
    let hash = sha256_hex(token);

    // DB errors here are deliberately masked as 401 rather than 503: an
    // unauthenticated caller does not get to learn the auth backend is
    // ailing. Operators investigating a 401 storm should pivot on the
    // `auth.lookup_failed` tracing event below, not on HTTP status.
    let project_id = match state.storage.api_keys().lookup_active_project(&hash).await {
        Ok(Some(pid)) => pid,
        Ok(None) => return Err(StatusCode::UNAUTHORIZED),
        Err(err) => {
            tracing::warn!(
                event = "auth.lookup_failed",
                error = %err,
                "api_keys lookup failed; treating as 401"
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    request
        .extensions_mut()
        .insert(AuthenticatedProject(project_id));

    let response = next.run(request).await;

    // Touch last_used_at AFTER the response. A failure here doesn't change
    // the auth outcome — it's just an audit-quality update.
    //
    // Spawn semantics: one tokio task per authenticated request. The task
    // itself is a few hundred bytes; under DB pool exhaustion the queue of
    // pending tasks is bounded by the request rate × time the pool spends
    // exhausted, which the storage pool's `max_connections` caps in
    // practice. If a Phase 4 multi-host deployment changes that calculus
    // (large fan-out, no shared pool), reconsider — at that point a
    // bounded JoinSet or a periodic batched UPDATE becomes the right fix.
    let storage = state.storage.clone();
    tokio::spawn(async move {
        if let Err(err) = storage.api_keys().touch_last_used(&hash).await {
            tracing::debug!(
                event = "auth.touch_failed",
                error = %err,
                "last_used_at update failed (non-fatal)"
            );
        }
    });

    Ok(response)
}

/// Header carrying the GitHub login of the operator performing the action
/// (Story 48.4). Forwarded by the anseo-web BFF; recorded on event rows so
/// every operator mutation is attributable. Optional — handlers fall back to
/// the `operator` body field / `"operator"` sentinel when absent.
pub const OPERATOR_ACTOR_HEADER: &str = "X-Anseo-Operator-Actor";

/// Env var carrying the single global operator-admin API key (Story 48.4).
/// Constant-time compared by [`require_operator_key`]. SINGLE-OPERATOR MODEL:
/// there is exactly one operator credential for the whole core API. The richer
/// `api_keys` scope-column approach (per-key operator scope) is deferred to
/// 49.x; this env-gate is the minimum that keeps tenant project keys out of the
/// `/v1/operator/*` surface.
pub const OPERATOR_API_KEY_ENV: &str = "ANSEO_OPERATOR_API_KEY";

/// Marker inserted into request extensions once the operator key has been
/// verified, carrying the resolved actor login for the handlers.
#[derive(Debug, Clone)]
pub struct AuthenticatedOperator {
    /// GitHub login of the operator, read from [`OPERATOR_ACTOR_HEADER`], or
    /// `None` when the header was absent.
    pub actor: Option<String>,
}

/// Constant-time equality over the two byte slices. Returns false on a length
/// mismatch (the length itself is not secret here — the key value is).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    if a.len() != b.len() {
        return false;
    }
    a.ct_eq(b).into()
}

/// Constant-time string equality. Public so the operator-entity erase
/// confirm-token verification (Story 48.4) compares its HMAC without a timing
/// leak through the same primitive used for the operator key.
pub fn ct_eq_str(a: &str, b: &str) -> bool {
    ct_eq(a.as_bytes(), b.as_bytes())
}

/// Auth gate for the `/v1/operator/*` sub-router (Story 48.4).
///
/// Verifies the presented `X-Anseo-API-Key` against the configured
/// `ANSEO_OPERATOR_API_KEY` env (constant-time). This is a DISTINCT credential
/// from tenant project API keys: a valid project key does NOT pass here (it is
/// not equal to the operator key), so tenant keys can never reach the operator
/// surface — exactly the isolation Story 48.4 requires.
///
/// * No `ANSEO_OPERATOR_API_KEY` configured → `503` (the surface is disabled;
///   we do not silently allow access).
/// * Missing / malformed key header → `401`.
/// * Key present but not equal to the operator key → `403`.
pub async fn require_operator_key(
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let configured = std::env::var(OPERATOR_API_KEY_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let Some(configured) = configured else {
        tracing::warn!(
            event = "operator_auth.not_configured",
            "{} is unset; the /v1/operator/* surface is disabled (503)",
            OPERATOR_API_KEY_ENV
        );
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    };

    let anseo_header: HeaderName = HeaderName::from_static("x-anseo-api-key");
    let legacy_header: HeaderName = HeaderName::from_static("x-opengeo-api-key");
    let presented = request
        .headers()
        .get(&anseo_header)
        .or_else(|| request.headers().get(&legacy_header))
        .and_then(|v| v.to_str().ok());

    let Some(presented) = extract_api_key(presented) else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if !ct_eq(presented.as_bytes(), configured.as_bytes()) {
        // A wrong key — including a perfectly valid TENANT project key — is a
        // 403: authenticated shape, but not authorized for the operator surface.
        tracing::warn!(
            event = "operator_auth.rejected",
            "presented key did not match the operator credential"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    let actor_header: HeaderName = HeaderName::from_static("x-anseo-operator-actor");
    let actor = request
        .headers()
        .get(&actor_header)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);

    request
        .extensions_mut()
        .insert(AuthenticatedOperator { actor });

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_core::api_key::{extract_api_key, generate, looks_like_key, sha256_hex};

    #[test]
    fn operator_key_constant_time_compare() {
        assert!(ct_eq(b"ogeo_secret_operator", b"ogeo_secret_operator"));
        // A valid-shaped tenant project key is NOT the operator key.
        assert!(!ct_eq(b"ogeo_tenant_project_key", b"ogeo_secret_operator"));
        // Length mismatch rejected.
        assert!(!ct_eq(b"short", b"longer-value"));
    }

    #[test]
    fn operator_header_constants_match_spec() {
        assert_eq!(OPERATOR_ACTOR_HEADER, "X-Anseo-Operator-Actor");
        assert_eq!(OPERATOR_API_KEY_ENV, "ANSEO_OPERATOR_API_KEY");
    }

    #[test]
    fn header_extraction_chain_handles_missing_header() {
        // No X-OpenGEO-API-Key header → extract_api_key returns None → 401.
        assert!(extract_api_key(None).is_none());
    }

    #[test]
    fn header_extraction_chain_handles_empty_header() {
        assert!(extract_api_key(Some("")).is_none());
        assert!(extract_api_key(Some("   ")).is_none());
    }

    #[test]
    fn header_extraction_chain_rejects_malformed_token_before_db() {
        // Header carries a token, but looks_like_key fails. Middleware must
        // 401 before any DB lookup.
        let token = extract_api_key(Some("not-our-format")).unwrap();
        assert!(!looks_like_key(token));
    }

    #[test]
    fn header_extraction_chain_accepts_valid_token() {
        let key = generate();
        let token = extract_api_key(Some(&key.plaintext)).unwrap();
        assert!(looks_like_key(token));
        assert_eq!(sha256_hex(token), key.sha256_hash);
    }

    #[test]
    fn header_name_constant_matches_spec() {
        assert_eq!(API_KEY_HEADER, "X-Anseo-API-Key");
    }
}
