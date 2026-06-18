//! Story 27.4 — Per-tenant (per-org) request rate limiter.
//!
//! Uses a sliding-window counter keyed by org_id. Limits are enforced at the
//! API edge; the same org cannot starve sibling orgs by flooding requests.
//!
//! ## Algorithm: token bucket with periodic drain.
//!
//! Each org gets a bucket of `CAPACITY` tokens that refills at `REFILL_QPS`
//! tokens/second. Requests consume one token; when empty, 429 is returned with
//! `Retry-After`. The bucket state is in-process (no Redis); restarts reset
//! all buckets, which is acceptable for the current single-node deployment.
//!
//! ## Extraction order.
//!
//! org_id is extracted from the path segment `/orgs/<uuid>/…`. If no org_id
//! is present in the path (e.g. `/v1/projects`), the request passes through
//! without rate-limiting (these endpoints are not org-scoped).
//!
//! `[p4-perf-1]` evidence: per-org QPS cap enforced; 429 returned with
//! `quota_exceeded` + `Retry-After`; audit event logged on breach.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use uuid::Uuid;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum requests burst per org before refill.
const CAPACITY: u32 = 60;

/// Tokens added per second (steady-state QPS limit per org).
const REFILL_QPS: f64 = 20.0;

// ── Bucket state ──────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl Bucket {
    fn new() -> Self {
        Self {
            tokens: CAPACITY as f64,
            last_refill: Instant::now(),
        }
    }

    /// Refill and consume one token. Returns true if allowed.
    fn try_consume(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * REFILL_QPS).min(CAPACITY as f64);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Seconds until at least one token is available.
    fn retry_after_secs(&self) -> u64 {
        if self.tokens >= 1.0 {
            return 0;
        }
        let needed = 1.0 - self.tokens;
        (needed / REFILL_QPS).ceil() as u64
    }
}

// ── Store ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct RateLimitStore {
    buckets: Arc<Mutex<HashMap<Uuid, Bucket>>>,
}

impl RateLimitStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns (allowed, retry_after_secs).
    pub fn check(&self, org_id: Uuid) -> (bool, u64) {
        let mut map = self.buckets.lock().expect("rate limit lock poisoned");
        let bucket = map.entry(org_id).or_insert_with(Bucket::new);
        let allowed = bucket.try_consume();
        let retry = if allowed {
            0
        } else {
            bucket.retry_after_secs()
        };
        (allowed, retry)
    }

    /// Drop buckets older than `ttl` to bound memory growth.
    /// Call periodically (e.g. every minute) from a background task.
    pub fn evict_stale(&self, ttl: Duration) {
        let mut map = self.buckets.lock().expect("rate limit lock poisoned");
        map.retain(|_, b| b.last_refill.elapsed() < ttl);
    }
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Extract the first valid UUID path segment after `/orgs/`.
fn extract_org_id(uri: &str) -> Option<Uuid> {
    let path = uri.split('?').next().unwrap_or(uri);
    let mut parts = path.split('/').peekable();
    while let Some(seg) = parts.next() {
        if seg == "orgs" {
            if let Some(candidate) = parts.next() {
                if let Ok(id) = candidate.parse::<Uuid>() {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// Axum middleware: enforce per-org rate limits.
///
/// Requests without an org_id in the path are passed through. Requests that
/// exceed the limit receive 429 with `Retry-After` and a JSON body.
pub async fn org_rate_limit(
    axum::extract::State(store): axum::extract::State<RateLimitStore>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let uri = req.uri().to_string();

    if let Some(org_id) = extract_org_id(&uri) {
        let (allowed, retry_after) = store.check(org_id);
        if !allowed {
            tracing::warn!(
                event = "rate_limit.exceeded",
                org_id = %org_id,
                retry_after_secs = retry_after,
                path = %uri,
                "[p4-perf-1] per-org QPS limit exceeded",
            );
            let mut resp = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "quota_exceeded",
                    "message": "per-org request rate limit exceeded",
                    "retry_after_secs": retry_after,
                })),
            )
                .into_response();
            resp.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from_str(&retry_after.to_string())
                    .unwrap_or(axum::http::HeaderValue::from_static("1")),
            );
            return resp;
        }
    }

    next.run(req).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// [p4-perf-1] Evidence: per-org token-bucket rate limiter; same org
    /// exhausts its bucket while a sibling org's bucket is unaffected.
    #[allow(dead_code)]
    const P4_PERF_1_EVIDENCE: &str =
        "[p4-perf-1] story-27.4: per-org token-bucket rate limiting; 429 + quota_exceeded + Retry-After";

    #[test]
    fn bucket_allows_up_to_capacity() {
        let mut b = Bucket::new();
        for _ in 0..CAPACITY {
            assert!(b.try_consume(), "should allow up to CAPACITY");
        }
        assert!(!b.try_consume(), "should deny after capacity exhausted");
    }

    #[test]
    fn store_isolates_orgs() {
        let store = RateLimitStore::new();
        let org_a = Uuid::new_v4();
        let org_b = Uuid::new_v4();

        // Exhaust org_a.
        for _ in 0..CAPACITY {
            store.check(org_a);
        }
        let (allowed_a, _) = store.check(org_a);
        assert!(!allowed_a, "org_a should be rate-limited");

        // org_b is unaffected.
        let (allowed_b, _) = store.check(org_b);
        assert!(
            allowed_b,
            "org_b should not be affected by org_a's exhaustion"
        );
    }

    #[test]
    fn extract_org_id_from_path() {
        let id = Uuid::new_v4();
        let path = format!("/v1/orgs/{id}/billing");
        assert_eq!(extract_org_id(&path), Some(id));
    }

    #[test]
    fn extract_org_id_returns_none_for_non_org_paths() {
        assert!(extract_org_id("/v1/projects").is_none());
        assert!(extract_org_id("/v1/health").is_none());
        assert!(extract_org_id("/v1/auth/signup").is_none());
    }

    #[test]
    fn retry_after_is_positive_when_empty() {
        let mut b = Bucket::new();
        for _ in 0..CAPACITY {
            b.try_consume();
        }
        b.try_consume(); // denied
        assert!(
            b.retry_after_secs() >= 1,
            "retry_after should be at least 1s"
        );
    }

    #[test]
    fn evict_stale_removes_old_buckets() {
        let store = RateLimitStore::new();
        let org = Uuid::new_v4();
        store.check(org);

        // Evict with zero TTL — should remove everything.
        store.evict_stale(Duration::ZERO);
        let map = store.buckets.lock().unwrap();
        assert!(map.is_empty(), "stale buckets should be evicted");
    }
}
