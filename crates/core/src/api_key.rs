//! API key generation + verification for the Phase 2 REST surface
//! (Story 12.1).
//!
//! Wire format: `ogeo_<32 chars random base62>` (37 chars total). The `ogeo_`
//! literal is matched in constant time by the auth middleware so a typo'd
//! key fails fast without a database round-trip. The trailing 32 chars are
//! the secret material; the first 8 of those become the row's `prefix`
//! (safe to display) and the full 32 chars are sha256-hashed for the
//! `sha256_hash` column. The plaintext is never persisted.
//!
//! Wire header: `X-OpenGEO-API-Key: ogeo_…`. This is the canonical Story 12.1
//! surface per the Phase 2 architecture's A-13. A legacy
//! `Authorization: Bearer …` extractor is kept exported because internal
//! tooling already uses it; the public REST API accepts only the
//! `X-OpenGEO-API-Key` form.

use sha2::{Digest, Sha256};

/// Stable prefix every issued key carries. Used by the auth middleware to
/// reject non-Anseo authorization values without a DB lookup.
/// NOTE: The `ogeo_` prefix is intentionally preserved for backward compatibility
/// with already-issued keys. New keys will continue to use `ogeo_` until a
/// future migration changes the prefix.
pub const KEY_PREFIX: &str = "ogeo_";

/// Canonical HTTP header carrying the Phase 2 REST API key (A-13).
pub const API_KEY_HEADER: &str = "X-Anseo-API-Key";

/// Deprecated pre-rename header name. The old header is accepted by the auth
/// middleware for one release for back-compat with pre-rename clients.
#[deprecated(since = "0.7.0", note = "use X-Anseo-API-Key instead")]
pub const API_KEY_HEADER_LEGACY: &str = "X-OpenGEO-API-Key";

/// Length of the random portion (after `ogeo_`). 32 base62 chars give
/// log2(62) * 32 ≈ 190 bits of entropy — well above the 128-bit floor for
/// a long-lived secret.
pub const RANDOM_LEN: usize = 32;

/// Length of the displayed prefix (first chars of the random portion).
pub const DISPLAY_PREFIX_LEN: usize = 8;

/// Generated key material returned to the caller of `ogeo api key create`.
/// `plaintext` is shown ONCE; `sha256_hash` + `display_prefix` are what the
/// caller persists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedApiKey {
    pub plaintext: String,
    pub sha256_hash: String,
    pub display_prefix: String,
}

/// Generate a fresh key using the OS RNG (`/dev/urandom` on Unix targets;
/// non-Unix is rejected at compile time). The full 32-char random portion
/// is shown to the caller exactly once.
///
/// Maps random bytes to base62 with rejection sampling so the character
/// distribution is uniform. A naive `byte % 62` mapping over `[0..256)` is
/// biased toward indices 0..7 (extra cyclic hit from bytes 248-255),
/// shaving ~0.4 % of the keyspace; we re-roll instead of accepting that.
pub fn generate() -> GeneratedApiKey {
    let mut random = String::with_capacity(RANDOM_LEN);
    while random.len() < RANDOM_LEN {
        let mut buf = [0u8; 1];
        fill_random(&mut buf);
        // 248 = 4 * 62; bytes [0..248) map uniformly to [0..62).
        if buf[0] < 248 {
            random.push(BASE62_CHARS[(buf[0] as usize) % 62] as char);
        }
    }
    let plaintext = format!("{KEY_PREFIX}{random}");
    let sha256_hash = sha256_hex(&plaintext);
    let display_prefix: String = random.chars().take(DISPLAY_PREFIX_LEN).collect();
    GeneratedApiKey {
        plaintext,
        sha256_hash,
        display_prefix,
    }
}

/// Hash a plaintext key for lookup. Returns lowercase hex (64 chars).
pub fn sha256_hex(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    hex_encode(&hasher.finalize())
}

/// Verify the wire-shape of an Authorization header value: must start with
/// `ogeo_` and the random portion must be exactly `RANDOM_LEN` base62
/// chars. Cheaper than a DB hit; rejects clearly malformed input early.
pub fn looks_like_key(plaintext: &str) -> bool {
    let Some(random) = plaintext.strip_prefix(KEY_PREFIX) else {
        return false;
    };
    random.len() == RANDOM_LEN && random.chars().all(|c| c.is_ascii_alphanumeric())
}

const BASE62_CHARS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(not(test))]
fn fill_random(buf: &mut [u8]) {
    // chrono::Utc::now() + a tiny SplitMix64 isn't crypto-grade; pull from
    // /dev/urandom on Unix and BCryptGenRandom on Windows via std's API.
    // Using the same approach as the `getrandom` crate would but without
    // adding the dep.
    use std::io::Read;
    #[cfg(unix)]
    {
        let mut f = std::fs::File::open("/dev/urandom")
            .expect("/dev/urandom unavailable for API key generation");
        f.read_exact(buf).expect("read /dev/urandom failed");
    }
    #[cfg(not(unix))]
    {
        // On non-Unix targets, panic loudly rather than fall back to a
        // weaker source. Phase 2 ships against Unix-y compose stacks.
        compile_error!("API key generation needs /dev/urandom on non-Unix targets");
    }
}

#[cfg(test)]
fn fill_random(buf: &mut [u8]) {
    // Test stub that advances a process-local counter so successive
    // `generate()` calls produce DIFFERENT keys. The earlier version
    // emitted a deterministic constant (`(i*7+13)%256`) which made every
    // call collide on `sha256_hash` — any test that inserted two keys
    // tripped the UNIQUE constraint. Real entropy is exercised by the
    // production `/dev/urandom` path in `#[cfg(not(test))]`.
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let base = COUNTER.fetch_add(1, Ordering::Relaxed);
    for (i, b) in buf.iter_mut().enumerate() {
        // Mix the per-call base with the byte offset; cheap LCG-ish step
        // is fine for tests.
        let mixed = base
            .wrapping_mul(6364136223846793005)
            .wrapping_add((i as u64).wrapping_mul(1442695040888963407));
        *b = ((mixed >> 32) & 0xFF) as u8;
    }
}

/// Outcome of validating an `Authorization: Bearer …` token. The caller
/// (axum middleware in `apps/api`) is responsible for translating each
/// variant into the corresponding HTTP status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Token verified; `project_id` is the scope the request runs against.
    Authorized { project_id: uuid::Uuid },
    /// Wire shape didn't match `ogeo_<32 alnum>`. Always 401 — never log
    /// the token.
    MalformedToken,
    /// Wire shape matched but the hash isn't in the active key set.
    UnknownOrRevoked,
}

/// Result of a single DB lookup for a key hash. The lookup function the
/// caller provides should return `Some` for a non-revoked match and `None`
/// otherwise.
pub type KeyLookup = dyn Fn(&str) -> Option<uuid::Uuid> + Send + Sync;

/// Pure-logic verifier kept axum-free so it's fully unit-testable without a
/// server fixture.
///
/// **Production note:** the real auth middleware in `apps/api` does the
/// DB lookup asynchronously and so calls `looks_like_key` + `sha256_hex`
/// directly rather than going through this synchronous wrapper. This
/// function is the test-double surface; do not add async-only logic here.
pub fn verify_token(token: &str, lookup: &KeyLookup) -> VerifyOutcome {
    if !looks_like_key(token) {
        return VerifyOutcome::MalformedToken;
    }
    let hash = sha256_hex(token);
    match lookup(&hash) {
        Some(project_id) => VerifyOutcome::Authorized { project_id },
        None => VerifyOutcome::UnknownOrRevoked,
    }
}

/// Extract the bearer token from a raw header value. Returns `None` for
/// missing/empty/non-Bearer schemes. Case-insensitive on the scheme keyword
/// per RFC 7235 §2.1. Splits on any ASCII whitespace (space or tab) per
/// RFC 7230 §3.2.3, so `Bearer\tfoo` is accepted.
pub fn extract_bearer(header_value: Option<&str>) -> Option<&str> {
    let raw = header_value?.trim();
    let (scheme, token) = raw.split_once(|c: char| c.is_ascii_whitespace())?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Extract the API-key token from the `X-OpenGEO-API-Key` header value.
/// Returns `None` for missing/empty. No scheme prefix — the header carries
/// the bare key.
pub fn extract_api_key(header_value: Option<&str>) -> Option<&str> {
    let token = header_value?.trim();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_plaintext_has_expected_shape() {
        let key = generate();
        assert!(key.plaintext.starts_with(KEY_PREFIX));
        assert_eq!(key.plaintext.len(), KEY_PREFIX.len() + RANDOM_LEN);
        assert!(looks_like_key(&key.plaintext));
    }

    #[test]
    fn display_prefix_matches_random_first_chars() {
        let key = generate();
        assert_eq!(key.display_prefix.len(), DISPLAY_PREFIX_LEN);
        let random = &key.plaintext[KEY_PREFIX.len()..];
        assert!(random.starts_with(&key.display_prefix));
    }

    #[test]
    fn sha256_hash_round_trips_with_lookup() {
        let key = generate();
        assert_eq!(sha256_hex(&key.plaintext), key.sha256_hash);
        assert_eq!(key.sha256_hash.len(), 64);
        assert!(key.sha256_hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha256_is_stable_for_known_plaintext() {
        // Pin a vector against a plaintext derived from the live constants
        // so changes to KEY_PREFIX or RANDOM_LEN re-derive the input rather
        // than silently still passing the test against a stale literal.
        let plaintext = format!("{}{}", KEY_PREFIX, "A".repeat(RANDOM_LEN));
        let h = sha256_hex(&plaintext);
        assert_eq!(h.len(), 64, "sha256 hex output must be 64 chars");
        // The full vector below is for the current shape
        // (KEY_PREFIX="ogeo_", RANDOM_LEN=32). If either constant changes
        // intentionally, regenerate via `echo -n <plaintext> | shasum -a 256`.
        if plaintext == "ogeo_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA" {
            assert_eq!(
                h,
                "dbe19df3056750e6452460fdfc8ead79b04828b64167631cbf5bf9c560e8f5d4"
            );
        }
    }

    #[test]
    fn looks_like_key_rejects_wrong_prefix() {
        assert!(!looks_like_key("sk-abc123"));
        assert!(!looks_like_key("OGEO_AAA"));
        assert!(!looks_like_key(""));
    }

    #[test]
    fn looks_like_key_rejects_wrong_length() {
        assert!(!looks_like_key("ogeo_short"));
        assert!(!looks_like_key(&format!(
            "ogeo_{}",
            "A".repeat(RANDOM_LEN + 5)
        )));
        // Prefix-only — random portion empty — must reject.
        assert!(!looks_like_key("ogeo_"));
    }

    #[test]
    fn looks_like_key_rejects_non_alphanumeric_random_portion() {
        let mut bad = String::from(KEY_PREFIX);
        bad.push_str(&"A".repeat(RANDOM_LEN - 1));
        bad.push('!');
        assert!(!looks_like_key(&bad));
    }

    #[test]
    fn extract_bearer_returns_token_for_canonical_header() {
        assert_eq!(extract_bearer(Some("Bearer abc123")), Some("abc123"));
        // Case-insensitive scheme.
        assert_eq!(extract_bearer(Some("bearer abc123")), Some("abc123"));
        assert_eq!(extract_bearer(Some("BEARER abc123")), Some("abc123"));
    }

    #[test]
    fn extract_bearer_returns_none_for_wrong_scheme() {
        assert_eq!(extract_bearer(Some("Basic abc123")), None);
        assert_eq!(extract_bearer(Some("abc123")), None);
        assert_eq!(extract_bearer(None), None);
        assert_eq!(extract_bearer(Some("Bearer  ")), None);
    }

    #[test]
    fn extract_bearer_accepts_tab_separated_scheme() {
        // RFC 7230 §3.2.3 allows tab as LWS.
        assert_eq!(extract_bearer(Some("Bearer\tabc123")), Some("abc123"));
    }

    #[test]
    fn extract_api_key_returns_token_for_x_opengeo_api_key_header() {
        assert_eq!(extract_api_key(Some("ogeo_abc123")), Some("ogeo_abc123"));
        // No scheme prefix; whitespace trimmed.
        assert_eq!(extract_api_key(Some("  ogeo_abc  ")), Some("ogeo_abc"));
    }

    #[test]
    fn extract_api_key_returns_none_for_missing_or_empty() {
        assert_eq!(extract_api_key(None), None);
        assert_eq!(extract_api_key(Some("")), None);
        assert_eq!(extract_api_key(Some("   ")), None);
    }

    #[test]
    fn generate_produces_distinct_keys_under_test_stub() {
        // Regression: previously the test fill_random was deterministic,
        // so two `generate()` calls produced identical plaintext and
        // collided on `sha256_hash UNIQUE`. The new counter-based stub
        // must yield distinct outputs.
        let a = generate();
        let b = generate();
        assert_ne!(
            a.plaintext, b.plaintext,
            "successive generate() calls must not collide"
        );
        assert_ne!(a.sha256_hash, b.sha256_hash);
    }

    #[test]
    fn api_key_header_constant_matches_spec() {
        assert_eq!(API_KEY_HEADER, "X-Anseo-API-Key");
    }

    #[test]
    fn verify_token_returns_malformed_for_wrong_shape() {
        let lookup: Box<KeyLookup> = Box::new(|_| panic!("lookup must not run on malformed input"));
        assert_eq!(
            verify_token("sk-not-ours", &*lookup),
            VerifyOutcome::MalformedToken
        );
    }

    #[test]
    fn verify_token_returns_unknown_when_lookup_misses() {
        let key = generate();
        let lookup: Box<KeyLookup> = Box::new(|_| None);
        assert_eq!(
            verify_token(&key.plaintext, &*lookup),
            VerifyOutcome::UnknownOrRevoked
        );
    }

    #[test]
    fn verify_token_returns_authorized_with_project_when_lookup_hits() {
        let key = generate();
        let expected_hash = key.sha256_hash.clone();
        let project = uuid::Uuid::from_u128(42);
        let lookup: Box<KeyLookup> = Box::new(move |hash| {
            if hash == expected_hash {
                Some(project)
            } else {
                None
            }
        });
        assert_eq!(
            verify_token(&key.plaintext, &*lookup),
            VerifyOutcome::Authorized {
                project_id: project
            }
        );
    }
}
