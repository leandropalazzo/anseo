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

    match state
        .storage
        .site_events()
        .insert(
            &body.event_type,
            body.session_id,
            body.path.as_deref(),
            body.referrer.as_deref(),
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
}
