//! Public site-event ingest — Story 47.1 (Epic 47: Public Site Analytics).
//!
//! `POST /v1/site-events` is the single collector all client-side
//! instrumentation (47.2 / 47.3) posts to. It is **unauthenticated** (the
//! public site must not require an operator API key to fire a page view) and
//! mounted on the `v1_public_surface` chain in `apps/api/src/lib.rs`.
//!
//! # Privacy contract (architecture A2)
//!
//! * **No IP is ever persisted.** The per-IP rate limiter (60 req/min) keys an
//!   in-memory sliding window on a non-cryptographic hash of the request IP;
//!   that key lives only in process memory and is never written to the DB or a
//!   log. The DB row carries no IP column at all.
//! * **No user IDs / fingerprints.** `session_id` is an ephemeral per-visit UUID
//!   generated client-side, not linked to identity.
//! * Unknown `event_type`s are **silently dropped with 204** — the client never
//!   learns the allowlist through error probing.
//!
//! Behind a reverse proxy the client IP arrives in `X-Forwarded-For`; per the
//! story note we take the **rightmost** entry (the last/trusted hop) rather than
//! the spoofable leftmost. If no forwarded header is present we fall back to a
//! single shared bucket (acceptable for local dev binds).
//!
//! Dynamic sqlx only — no `query!` macros.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use anseo_storage::repositories::site_events::is_known_event_type;

use crate::AppState;

/// Rate-limit window: 60 events per minute per IP (AC-3).
const RATE_LIMIT_MAX: usize = 60;
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

/// Public ingest router. Mounted under `/v1` on the UNAUTHENTICATED public
/// surface (see `v1_public_surface` in `lib.rs`).
pub fn v1_router() -> Router<AppState> {
    Router::new().route("/site-events", post(ingest_site_event))
}

/// Request body for `POST /v1/site-events`. `referrer` is expected to be a bare
/// domain (the client normalizes); `path` a site-relative path. Everything else
/// goes into `properties`.
#[derive(Debug, Deserialize)]
pub struct SiteEventBody {
    pub event_type: String,
    pub session_id: uuid::Uuid,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub referrer: Option<String>,
    #[serde(default)]
    pub properties: Option<serde_json::Value>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Dimension normalization (trust boundary — privacy / data-poisoning defense)
// ─────────────────────────────────────────────────────────────────────────────
//
// `path` and `referrer` are copied verbatim into the durable aggregate rollup
// tables (`site_page_rollups` / `site_referrer_rollups`) and surfaced on the
// operator dashboard. This is an UNAUTHENTICATED public endpoint, so a malicious
// or buggy client can submit a full URL, query string, email, or arbitrary
// string. Epic 47 is privacy-SAFE analytics: we must never let raw, PII-bearing
// values become durable. So we normalize here, at the trust boundary, before the
// value is ever persisted. Anything that can't be reduced to a safe canonical
// form is bucketed to the `OTHER_BUCKET` sentinel rather than stored raw.

/// Max stored length for a normalized path or referrer. Bounds storage and
/// strips pathological long inputs.
const DIM_MAX_LEN: usize = 256;

/// Safe sentinel for inputs that can't be reduced to a privacy-safe canonical
/// value (e.g. a referrer containing an `@`, or an unparseable path).
const OTHER_BUCKET: &str = "(other)";

/// Truncate `s` to at most `max_bytes`, landing on a UTF-8 char boundary.
///
/// `String::truncate` panics if the byte index is not a char boundary, so a
/// public request carrying a long multi-byte string (e.g. a path of repeated
/// `é`) could otherwise crash the handler task (unauthenticated DoS). We instead
/// pick the largest byte index `<= max_bytes` that is a valid boundary and slice
/// there — never splitting a code point and never exceeding the byte cap.
fn truncate_on_char_boundary(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
}

/// Safe sentinel for a `properties.method` value that isn't in the closed verify
/// method enum (an arbitrary / poisoned string).
const METHOD_UNKNOWN: &str = "unknown";

/// Bucket a public-submitted verify `method` to the closed, non-PII enum the
/// dashboard understands.
///
/// `method` rides in the UNAUTHENTICATED `/v1/site-events` body, so a client can
/// submit `{"method":"jane@example.com"}` (or any long/poisoned string) and —
/// were it stored verbatim — it would surface as a method label on
/// `/v1/analytics/funnels`, violating the fixed-enum / non-PII contract. So we
/// reduce it here at the trust boundary to one of the canonical labels and store
/// ONLY the bucketed value:
///
///   * `dns` / `dns_txt`            → `dns`
///   * `email` / `email_magic_link` → `email`
///   * anything else                → `unknown`
///
/// The canonical verification methods (Story 43.2,
/// `anseo_storage::repositories::verification::VerificationMethod`) are
/// `dns_txt` and `email_magic_link`; the dashboard surfaces the short `dns` /
/// `email` labels, so we accept both spellings and normalize to the short form.
fn bucket_verify_method(raw: &str) -> &'static str {
    match raw.trim().to_ascii_lowercase().as_str() {
        "dns" | "dns_txt" => "dns",
        "email" | "email_magic_link" => "email",
        _ => METHOD_UNKNOWN,
    }
}

/// Rewrite `properties.method` (if present) to the closed verify-method enum
/// before the event is persisted. Leaves all other properties untouched. A
/// non-object `properties` (or one without a `method`) is returned unchanged.
fn sanitize_method_property(mut properties: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = properties.as_object_mut() {
        if let Some(method) = obj.get("method") {
            // Bucket any present `method` — including a non-string value, which
            // can't be a valid enum, so it collapses to `unknown`.
            let bucketed = method
                .as_str()
                .map(bucket_verify_method)
                .unwrap_or(METHOD_UNKNOWN);
            obj.insert(
                "method".to_string(),
                serde_json::Value::String(bucketed.to_string()),
            );
        }
    }
    properties
}

/// `true` if the string contains an ASCII control char (incl. NUL / newline /
/// tab) — never allowed in a stored dimension.
fn has_control_chars(s: &str) -> bool {
    s.chars().any(|c| c.is_control())
}

/// Normalize a submitted `path` to a privacy-safe, site-relative path.
///
/// * Strips a scheme+host if a full URL was sent (`https://x.com/a?b=c` → `/a`).
/// * Drops the query string and fragment (no PII / no full URLs in aggregates).
/// * Rejects control chars; caps length.
/// * Returns `None` for an empty/blank input (nothing to store); returns the
///   `OTHER_BUCKET` sentinel for anything that can't be normalized to a path.
fn normalize_path(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if has_control_chars(raw) {
        return Some(OTHER_BUCKET.to_string());
    }

    // If a full/absolute URL was submitted, keep only the path component.
    let path_part: &str = if let Some(rest) = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
    {
        // `rest` is `host[:port]/path?query#frag` — take from the first `/`.
        match rest.find('/') {
            Some(i) => &rest[i..],
            None => "/", // bare host, no path
        }
    } else if let Some(rest) = raw.strip_prefix("//") {
        // Protocol-relative URL (`//host/path`) — keep the path component.
        match rest.find('/') {
            Some(i) => &rest[i..],
            None => return Some(OTHER_BUCKET.to_string()), // bare host, no path
        }
    } else if raw.starts_with('/') {
        raw
    } else {
        // A bare token that isn't a site-relative path → not a path we trust.
        return Some(OTHER_BUCKET.to_string());
    };

    // Drop query string and fragment.
    let path_only = path_part
        .split(['?', '#'])
        .next()
        .unwrap_or("/");
    let path_only = if path_only.is_empty() { "/" } else { path_only };

    let mut out = path_only.to_string();
    truncate_on_char_boundary(&mut out, DIM_MAX_LEN);
    Some(out)
}

/// Normalize a submitted `referrer` to a bare registrable host/origin.
///
/// * Strips scheme, path, query, and fragment
///   (`https://news.ycombinator.com/x?y` → `news.ycombinator.com`).
/// * Drops anything that looks like an email / contains userinfo (any `@`) or is
///   an arbitrary string (no dot) → `OTHER_BUCKET`.
/// * Lowercases; rejects control chars; caps length.
/// * Returns `None` for empty/blank (direct visit — store nothing).
fn normalize_referrer(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if has_control_chars(raw) {
        return Some(OTHER_BUCKET.to_string());
    }

    // Any `@` means email-like or carries userinfo. A privacy-safe referrer is a
    // bare domain/origin and never contains userinfo, so we refuse to try to
    // salvage a host out of it — bucket the whole thing. This is what stops an
    // email (`jane@corp.com`) from being stored as `corp.com`.
    if raw.contains('@') {
        return Some(OTHER_BUCKET.to_string());
    }

    // Strip scheme.
    let after_scheme = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
        .or_else(|| raw.strip_prefix("//"))
        .unwrap_or(raw);

    // Cut at the first path / query / fragment separator → authority only.
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");

    let host_port = authority;

    // Drop the port.
    let host = host_port.split(':').next().unwrap_or("");
    let host = host.trim();

    // Reject anything that still isn't a plausible bare domain: must be
    // non-empty, contain a dot, contain no `@` (email-like), and only
    // host-legal characters. Otherwise bucket it.
    let looks_like_domain = !host.is_empty()
        && host.contains('.')
        && !host.contains('@')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !looks_like_domain {
        return Some(OTHER_BUCKET.to_string());
    }

    let mut out = host.to_ascii_lowercase();
    truncate_on_char_boundary(&mut out, DIM_MAX_LEN);
    Some(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// In-memory per-IP sliding-window rate limiter
// ─────────────────────────────────────────────────────────────────────────────

/// Process-local rate-limit state. The key is a non-cryptographic hash of the
/// request IP — it exists ONLY in memory and is never persisted (privacy A2).
/// Value is the timestamps of recent hits within the window.
fn rate_limiter() -> &'static Mutex<HashMap<u64, Vec<Instant>>> {
    static LIMITER: OnceLock<Mutex<HashMap<u64, Vec<Instant>>>> = OnceLock::new();
    LIMITER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record a hit for `ip_key` and return `true` if it is allowed (i.e. the
/// caller has made ≤ `RATE_LIMIT_MAX` requests in the trailing window).
/// Pure sliding window: prunes timestamps older than the window on each call.
fn check_rate_limit(ip_key: u64, now: Instant) -> bool {
    let mut map = rate_limiter().lock().unwrap_or_else(|e| e.into_inner());
    let hits = map.entry(ip_key).or_default();
    hits.retain(|t| now.duration_since(*t) < RATE_LIMIT_WINDOW);
    if hits.len() >= RATE_LIMIT_MAX {
        false
    } else {
        hits.push(now);
        true
    }
}

/// Derive an ephemeral, in-memory-only rate-limit key from the request IP.
///
/// Prefers `X-Forwarded-For` (rightmost / trusted hop), then the direct socket
/// peer. Returns a hash so the raw IP is never even held as a map key. If no IP
/// can be determined, returns `0` (a shared dev bucket).
fn ip_key(headers: &HeaderMap, peer: Option<std::net::SocketAddr>) -> u64 {
    let ip_str: Option<String> = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| {
            raw.split(',')
                .map(str::trim)
                .rfind(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .or_else(|| peer.map(|p| p.ip().to_string()));

    match ip_str {
        Some(ip) => {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            ip.hash(&mut h);
            h.finish()
        }
        None => 0,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler
// ─────────────────────────────────────────────────────────────────────────────

/// `POST /v1/site-events`.
///
/// Returns `204 No Content` for accepted **and** silently-dropped (unknown
/// type) events; `429 Too Many Requests` when the per-IP window is exceeded;
/// `400 Bad Request` only for a structurally invalid body (bad JSON / missing
/// required field), which Axum's `Json` extractor handles before this runs.
async fn ingest_site_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
    Json(body): Json<SiteEventBody>,
) -> StatusCode {
    // Rate-limit at the edge. The IP key is ephemeral (in-memory only).
    let peer = connect_info.map(|ci| ci.0);
    let key = ip_key(&headers, peer);
    if !check_rate_limit(key, Instant::now()) {
        return StatusCode::TOO_MANY_REQUESTS;
    }

    // Silent drop for unknown event types (prevents allowlist enumeration).
    if !is_known_event_type(&body.event_type) {
        return StatusCode::NO_CONTENT;
    }

    let properties = body
        .properties
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    // Bucket `properties.method` to the closed verify-method enum at the trust
    // boundary so a poisoned/arbitrary string can never surface as a method
    // label on /v1/analytics/funnels (Finding 1).
    let properties = sanitize_method_property(properties);

    // Normalize the public-submitted dimensions at the trust boundary BEFORE they
    // are persisted. This is the privacy/data-poisoning defense: a full URL,
    // query string, email-like referrer, or arbitrary string never becomes
    // durable in the aggregate tables (Epic 47 = privacy-safe analytics).
    let path = body.path.as_deref().and_then(normalize_path);
    let referrer = body.referrer.as_deref().and_then(normalize_referrer);

    match state
        .storage
        .site_events()
        .insert(
            &body.event_type,
            body.session_id,
            path.as_deref(),
            referrer.as_deref(),
            &properties,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            // Never surface storage internals to the public client; log + 204
            // so instrumentation failures can't break the public site or leak
            // error detail. The event is simply lost (best-effort analytics).
            tracing::warn!(
                event = "site_events.insert_failed",
                error = %e,
                "failed to persist site event"
            );
            StatusCode::NO_CONTENT
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_allows_up_to_max_then_blocks() {
        let now = Instant::now();
        // Use a key unlikely to collide with other tests.
        let key = 0xC0FFEE_u64;
        for _ in 0..RATE_LIMIT_MAX {
            assert!(check_rate_limit(key, now));
        }
        // 61st within the window is rejected.
        assert!(!check_rate_limit(key, now));
        // A request after the window slides forward is allowed again.
        let later = now + RATE_LIMIT_WINDOW + Duration::from_secs(1);
        assert!(check_rate_limit(key, later));
    }

    #[test]
    fn ip_key_prefers_rightmost_forwarded_hop() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "1.1.1.1, 2.2.2.2, 3.3.3.3".parse().unwrap(),
        );
        let rightmost = ip_key(&headers, None);

        let mut only_right = HeaderMap::new();
        only_right.insert("x-forwarded-for", "3.3.3.3".parse().unwrap());
        assert_eq!(rightmost, ip_key(&only_right, None));

        // A different rightmost hop yields a different key.
        let mut other = HeaderMap::new();
        other.insert("x-forwarded-for", "1.1.1.1, 9.9.9.9".parse().unwrap());
        assert_ne!(rightmost, ip_key(&other, None));
    }

    #[test]
    fn ip_key_falls_back_to_zero_without_ip() {
        assert_eq!(ip_key(&HeaderMap::new(), None), 0);
    }

    // ── Dimension normalization (trust boundary, Finding 1) ────────────────────

    #[test]
    fn normalize_path_keeps_site_relative_drops_query_and_fragment() {
        assert_eq!(normalize_path("/leaderboard"), Some("/leaderboard".into()));
        assert_eq!(normalize_path("/"), Some("/".into()));
        // Query string + fragment are dropped (no PII / no query strings stored).
        assert_eq!(
            normalize_path("/search?q=secret&email=a@b.com#frag"),
            Some("/search".into())
        );
    }

    #[test]
    fn normalize_path_strips_scheme_and_host_from_full_url() {
        // A submitted FULL URL with a query string must reduce to the bare path,
        // never stored raw. (Finding 1 required case.)
        assert_eq!(
            normalize_path("https://evil.example.com/account?token=abc123&user=jane@corp.com"),
            Some("/account".into())
        );
        // Fragment-carried token on a full URL must be stripped too (Finding 2).
        assert_eq!(
            normalize_path("https://example.com/account#token=abc"),
            Some("/account".into())
        );
        // Fragment on a site-relative path is stripped.
        assert_eq!(normalize_path("/account#token=abc"), Some("/account".into()));
        // Bare host with no path → root.
        assert_eq!(normalize_path("https://evil.example.com"), Some("/".into()));
        // Protocol-relative URL.
        assert_eq!(normalize_path("//cdn.example.com/a/b?x=1"), Some("/a/b".into()));
    }

    #[test]
    fn normalize_path_buckets_arbitrary_or_unsafe_input() {
        // Arbitrary non-path token → sentinel, never stored raw.
        assert_eq!(normalize_path("javascript:alert(1)"), Some("(other)".into()));
        assert_eq!(normalize_path("just some text"), Some("(other)".into()));
        // Control chars → sentinel.
        assert_eq!(normalize_path("/a\nb"), Some("(other)".into()));
        // Empty/blank → nothing to store.
        assert_eq!(normalize_path("   "), None);
        assert_eq!(normalize_path(""), None);
    }

    #[test]
    fn normalize_referrer_reduces_to_bare_domain() {
        assert_eq!(normalize_referrer("https://www.google.com/search?q=x"), Some("www.google.com".into()));
        assert_eq!(normalize_referrer("google.com"), Some("google.com".into()));
        // Port, path, query, fragment all stripped; lowercased.
        assert_eq!(
            normalize_referrer("https://News.YCombinator.com:443/item?id=1#c"),
            Some("news.ycombinator.com".into())
        );
        // Any userinfo (`@`) is refused outright (not salvaged into a host).
        assert_eq!(
            normalize_referrer("https://user:pass@news.ycombinator.com/item"),
            Some("(other)".into())
        );
    }

    // ── Verify-method bucketing (trust boundary, Finding 1) ────────────────────

    #[test]
    fn bucket_verify_method_maps_to_closed_enum() {
        // Canonical short + long spellings normalize to the short dashboard label.
        assert_eq!(bucket_verify_method("dns"), "dns");
        assert_eq!(bucket_verify_method("dns_txt"), "dns");
        assert_eq!(bucket_verify_method("email"), "email");
        assert_eq!(bucket_verify_method("email_magic_link"), "email");
        // Case / whitespace insensitive.
        assert_eq!(bucket_verify_method("  DNS_TXT "), "dns");
        // Anything else — incl. a poisoned email / long string — is `unknown`,
        // never surfaced verbatim.
        assert_eq!(bucket_verify_method("jane@example.com"), "unknown");
        assert_eq!(bucket_verify_method(&"x".repeat(5000)), "unknown");
        assert_eq!(bucket_verify_method(""), "unknown");
    }

    #[test]
    fn sanitize_method_property_rewrites_poisoned_method() {
        // A poisoned email-like method string is bucketed to `unknown` before
        // the event is persisted — never stored raw.
        let props = serde_json::json!({"method": "jane@example.com", "step": "consent"});
        let out = sanitize_method_property(props);
        assert_eq!(out["method"], serde_json::json!("unknown"));
        // Unrelated properties are left intact.
        assert_eq!(out["step"], serde_json::json!("consent"));

        // A canonical method is normalized to its short label.
        let out = sanitize_method_property(serde_json::json!({"method": "dns_txt"}));
        assert_eq!(out["method"], serde_json::json!("dns"));

        // A non-string method can't be a valid enum → `unknown`.
        let out = sanitize_method_property(serde_json::json!({"method": 12345}));
        assert_eq!(out["method"], serde_json::json!("unknown"));

        // No `method` key → untouched.
        let out = sanitize_method_property(serde_json::json!({"other": 1}));
        assert!(out.get("method").is_none());
    }

    #[test]
    fn normalize_referrer_drops_email_like_and_arbitrary_strings() {
        // An email-like referrer must NOT be stored raw — bucketed to sentinel.
        // (Finding 1 required case.)
        assert_eq!(normalize_referrer("jane.doe@corp.com"), Some("(other)".into()));
        // Arbitrary string with no dot → sentinel.
        assert_eq!(normalize_referrer("not a url"), Some("(other)".into()));
        // Control chars → sentinel.
        assert_eq!(normalize_referrer("evil\u{0000}.com"), Some("(other)".into()));
        // Empty (direct visit) → store nothing.
        assert_eq!(normalize_referrer(""), None);
        assert_eq!(normalize_referrer("   "), None);
    }

    /// Finding 2 — a public request carrying a long multi-byte path/referrer must
    /// NOT panic. `String::truncate` panics on a non-char-boundary byte index, so
    /// truncating a string of repeated `é` (2 bytes each) at DIM_MAX_LEN=256 would
    /// land mid-character. The safe truncation must instead return a valid UTF-8
    /// string whose byte length is <= DIM_MAX_LEN and never split a code point.
    #[test]
    fn normalize_handles_long_non_ascii_without_panic() {
        // 257 bytes: "/" (1 byte) + 128 × "é" (2 bytes each = 256 bytes).
        let long_path = format!("/{}", "é".repeat(128));
        assert_eq!(long_path.len(), 257);
        let out = normalize_path(&long_path).expect("non-empty path normalizes");
        // No panic; result is valid UTF-8, within the byte cap, and on a boundary.
        assert!(out.len() <= DIM_MAX_LEN, "byte length must respect cap: {}", out.len());
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
        assert!(out.starts_with('/'));
        // 256 is odd-vs-the-2-byte boundary after the leading '/', so the safe
        // truncation backs off to 255 bytes (1 + 127 × 2).
        assert_eq!(out.len(), 255);
        assert_eq!(out.chars().filter(|c| *c == 'é').count(), 127);

        // A long multi-byte referrer host must also not panic. Referrer hosts are
        // ASCII-only after validation, so a non-ASCII referrer buckets safely.
        let long_ref = format!("{}.com", "é".repeat(200));
        let rout = normalize_referrer(&long_ref).expect("non-empty referrer normalizes");
        assert!(rout.len() <= DIM_MAX_LEN);
        assert!(std::str::from_utf8(rout.as_bytes()).is_ok());
    }

    #[test]
    fn truncate_on_char_boundary_never_splits_code_point() {
        // Exactly at cap → unchanged.
        let mut s = "a".repeat(DIM_MAX_LEN);
        truncate_on_char_boundary(&mut s, DIM_MAX_LEN);
        assert_eq!(s.len(), DIM_MAX_LEN);

        // Multi-byte string just over the cap backs off to a boundary.
        let mut s = "é".repeat(200); // 400 bytes
        truncate_on_char_boundary(&mut s, DIM_MAX_LEN);
        assert!(s.len() <= DIM_MAX_LEN);
        assert!(std::str::from_utf8(s.as_bytes()).is_ok());
        // 256 is even and each 'é' is 2 bytes, so 128 chars (256 bytes) fit exactly.
        assert_eq!(s.len(), 256);
    }
}
