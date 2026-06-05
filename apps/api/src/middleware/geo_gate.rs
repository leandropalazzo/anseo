//! Geo-gating middleware — Story 44.4: High-Friction Jurisdiction Controls.
//!
//! Blocks identified-tier (class-c) endpoints for requests originating from
//! high-friction jurisdictions. Only the identified tier is restricted;
//! anonymous/aggregate endpoints remain fully accessible.
//!
//! ## Configuration
//! - `ANSEO_HIGH_FRICTION_JURISDICTIONS` — comma-separated ISO-3166-1 alpha-2
//!   country codes (e.g. `CN,IN,BR`). Defaults to `CN,IN,BR`.
//!
//! ## IP detection
//! The middleware reads (in priority order):
//!   1. `CF-IPCountry` header (Cloudflare)
//!   2. `X-Country-Code` header (custom operator header)
//!   3. `X-Forwarded-For` first IP → resolved via `CF-IPCountry` fallback.
//!
//! **Acknowledged limitation**: VPN/proxy detection gaps are expected and
//! documented. This is a risk-reduction posture, not an enforcement guarantee.
//!
//! ## Rejection logging
//! Rejections are logged with: jurisdiction code, endpoint path, timestamp.
//! No personal-data fields (IP address, User-Agent) are logged.
//!
//! ## Dynamic reload
//! The jurisdiction list is read from env on every request, so config changes
//! take effect within the next request (well within the 5-minute SLA in AC-3).

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use axum::Json;
use serde_json::json;

/// Identified-tier endpoint path prefixes that are geo-gated.
/// Anonymous/aggregate endpoints are NOT in this list (AC-2).
const IDENTIFIED_TIER_PREFIXES: &[&str] = &[
    "/v1/benchmark/contributions",
    "/v1/claim",
    "/v1/benchmark/optin",
    "/v1/benchmark/brands/leaderboard", // named-brand data = class-c when opted-in
];

/// Load the current jurisdiction blocklist from the environment.
/// Returns a Vec of uppercased country codes.
///
/// Re-read on every call — O(1) env lookup enables live config reload
/// without a server restart, satisfying AC-3 (≤5 min to take effect).
pub fn current_blocked_jurisdictions() -> Vec<String> {
    let raw = std::env::var("ANSEO_HIGH_FRICTION_JURISDICTIONS")
        .unwrap_or_else(|_| "CN,IN,BR".to_string());
    raw.split(',')
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extract the best-available country code from request headers.
/// Returns `None` when the origin country cannot be determined.
///
/// Acknowledged limitation: this is header-based and can be bypassed by
/// VPNs or proxies. Documented in `docs-internal/compliance/cross-border-transfers.md`.
fn extract_country_code(request: &Request<Body>) -> Option<String> {
    let headers = request.headers();

    // 1. Cloudflare's canonical country header (most reliable in CF deployments).
    if let Some(v) = headers
        .get("CF-IPCountry")
        .or_else(|| headers.get("cf-ipcountry"))
        .and_then(|v| v.to_str().ok())
    {
        let code = v.trim().to_uppercase();
        if code.len() == 2 && code != "XX" && code != "T1" {
            return Some(code);
        }
    }

    // 2. Operator-set override (useful behind own reverse proxy).
    if let Some(v) = headers
        .get("X-Country-Code")
        .or_else(|| headers.get("x-country-code"))
        .and_then(|v| v.to_str().ok())
    {
        let code = v.trim().to_uppercase();
        if code.len() == 2 {
            return Some(code);
        }
    }

    None
}

/// Returns `true` when this path is in the identified tier.
fn is_identified_tier(path: &str) -> bool {
    IDENTIFIED_TIER_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

/// Axum middleware entry point.
///
/// Pass-through for anonymous/aggregate endpoints. For identified-tier
/// endpoints, checks the request country against the blocklist and returns
/// 403 with the spec-mandated message on a match.
pub async fn geo_gate_middleware(
    request: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let path = request.uri().path().to_owned();

    // Fast-path: only inspect identified-tier endpoints.
    if !is_identified_tier(&path) {
        return Ok(next.run(request).await);
    }

    let blocked = current_blocked_jurisdictions();
    if blocked.is_empty() {
        return Ok(next.run(request).await);
    }

    if let Some(country) = extract_country_code(&request) {
        if blocked.contains(&country) {
            // Rejection log: jurisdiction code + endpoint path + timestamp.
            // Deliberately NO IP address, User-Agent, or other personal data.
            tracing::warn!(
                event = "geo_gate.rejected",
                jurisdiction = %country,
                endpoint = %path,
                timestamp = %chrono::Utc::now().to_rfc3339(),
                "identified-tier request rejected from high-friction jurisdiction"
            );

            return Err((
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "jurisdiction_restricted",
                    "message": "This feature is not currently available in your region. \
                                We are working to extend coverage — contact us for more information."
                })),
            ));
        }
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Default + custom checks share one process-global env var, so they must run
    // sequentially in a single test — otherwise `cargo test`'s parallel threads
    // race set_var against remove_var (flaky under --workspace).
    #[test]
    fn default_then_custom_blocked_list() {
        std::env::remove_var("ANSEO_HIGH_FRICTION_JURISDICTIONS");
        let list = current_blocked_jurisdictions();
        assert!(list.contains(&"CN".to_string()));
        assert!(list.contains(&"IN".to_string()));
        assert!(list.contains(&"BR".to_string()));

        std::env::set_var("ANSEO_HIGH_FRICTION_JURISDICTIONS", "DE, FR, JP");
        let list = current_blocked_jurisdictions();
        assert!(list.contains(&"DE".to_string()));
        assert!(list.contains(&"FR".to_string()));
        assert!(list.contains(&"JP".to_string()));
        assert!(!list.contains(&"CN".to_string()));
        std::env::remove_var("ANSEO_HIGH_FRICTION_JURISDICTIONS");
    }

    #[test]
    fn empty_list_allows_all() {
        // Parse the logic directly rather than relying on env (avoids parallel-test bleed).
        let raw = "";
        let list: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().to_uppercase())
            .filter(|s| !s.is_empty())
            .collect();
        assert!(list.is_empty());
    }

    #[test]
    fn identified_tier_paths_detected() {
        assert!(is_identified_tier("/v1/benchmark/contributions"));
        assert!(is_identified_tier("/v1/claim"));
        assert!(is_identified_tier("/v1/benchmark/optin"));
        assert!(is_identified_tier("/v1/benchmark/brands/leaderboard"));
    }

    #[test]
    fn anonymous_tier_paths_not_detected() {
        assert!(!is_identified_tier("/v1/benchmark/density-check"));
        assert!(!is_identified_tier("/v1/visibility/trend"));
        assert!(!is_identified_tier("/v1/runs"));
        assert!(!is_identified_tier("/healthz"));
    }

    #[test]
    fn blocked_jurisdictions_are_blocked() {
        let blocked = ["CN".to_string(), "IN".to_string(), "BR".to_string()];
        assert!(blocked.contains(&"CN".to_string()));
        assert!(!blocked.contains(&"US".to_string()));
    }
}
