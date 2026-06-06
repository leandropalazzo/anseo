//! Domain-verification repository — Story 43.2.
//!
//! Verification is a STATE MACHINE persisted append-only in
//! `verification_attempts`. This repo owns:
//!
//!   * minting + storing a hashed challenge token (the raw token lives only in
//!     the DNS TXT record or the magic-link URL),
//!   * the rate-limit window count (CC-NFR4 / AC-6),
//!   * single-use consume-on-verify (replay rejected → 401 at the API layer),
//!   * the revocation scan that drives the daily re-verify job (AC-5).
//!
//! Token hashing reuses the same posture as `anseo-comms`: `sha256(raw)` is the
//! DB lookup key, comparison is constant-time, and the raw token is never
//! persisted. We DO NOT depend on `anseo-comms` here to keep storage free of a
//! cycle; the hashing primitives are small and duplicated intentionally.
//!
//! DNS resolution is behind the pluggable [`TxtResolver`] trait so tests use an
//! in-memory mock and never touch the network.

use std::collections::HashMap;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::Row as _;
use subtle::ConstantTimeEq;

use crate::error::Error;

/// Verification method. DNS-TXT is the primary / higher-trust path and the only
/// method that qualifies for ranked-leaderboard badges (NFR8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationMethod {
    DnsTxt,
    EmailMagicLink,
}

impl VerificationMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            VerificationMethod::DnsTxt => "dns_txt",
            VerificationMethod::EmailMagicLink => "email_magic_link",
        }
    }

    /// Token validity window for this method. DNS propagation needs hours;
    /// magic links are short-lived (AC-1 / AC-3).
    pub fn ttl(self) -> chrono::Duration {
        match self {
            VerificationMethod::DnsTxt => chrono::Duration::hours(48),
            VerificationMethod::EmailMagicLink => chrono::Duration::minutes(30),
        }
    }

    /// True if this method qualifies for ranked-leaderboard placement (NFR8).
    pub fn qualifies_for_ranking(self) -> bool {
        matches!(self, VerificationMethod::DnsTxt)
    }
}

/// The current attestation text the claimant must agree to (AC-7).
pub const ATTESTATION_TEXT: &str =
    "I am authorized to act for and on behalf of the owner/operator of this domain.";
/// Attestation version stored with each attempt; bump when the text changes.
pub const ATTESTATION_VERSION: &str = "v1-2026-06-06";

/// DNS challenge label prefix and TXT value prefix (AC-1). The exact record is
/// `_anseo-challenge.<domain> IN TXT "anseo-verify=<token>"`.
pub const CHALLENGE_LABEL_PREFIX: &str = "_anseo-challenge";
pub const CHALLENGE_VALUE_PREFIX: &str = "anseo-verify=";

/// Max verification attempts per domain per rolling hour (CC-NFR4 / AC-6).
pub const RATE_LIMIT_PER_HOUR: i64 = 5;

/// A freshly-minted challenge. The `raw_token` is returned to the caller (to
/// embed in the TXT record or the magic-link URL); only `token_hash` is stored.
#[derive(Debug, Clone)]
pub struct MintedChallenge {
    pub id: uuid::Uuid,
    pub raw_token: String,
    pub method: VerificationMethod,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl MintedChallenge {
    /// The fully-qualified DNS challenge name for a domain.
    pub fn dns_record_name(domain: &str) -> String {
        format!("{CHALLENGE_LABEL_PREFIX}.{domain}")
    }

    /// The exact TXT value the operator must publish.
    pub fn dns_record_value(&self) -> String {
        format!("{CHALLENGE_VALUE_PREFIX}{}", self.raw_token)
    }
}

/// A stored attempt row, as needed by the verify + revocation paths.
#[derive(Debug, Clone)]
pub struct AttemptRecord {
    pub id: uuid::Uuid,
    pub domain: String,
    pub method: String,
    pub token_hash: String,
    pub state: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub consumed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ─────────────────────────────────────────────────────────────────────────────
// DNS resolver seam (pluggable; in-memory mock for tests — NO real network)
// ─────────────────────────────────────────────────────────────────────────────

/// Pluggable DNS TXT resolver. The production path performs DNSSEC-validated
/// resolution (AC-2); tests use [`MockTxtResolver`] and never touch the network.
#[async_trait]
pub trait TxtResolver: Send + Sync {
    /// Resolve all TXT record strings for `name` (e.g.
    /// `_anseo-challenge.example.com`). Returns an empty vec when the name has
    /// no TXT records.
    async fn lookup_txt(&self, name: &str) -> Result<Vec<String>, ResolveError>;
}

/// Errors a resolver can raise. `Transient` is retried by the caller; `NotFound`
/// is treated as "record absent" (drives revocation in the daily job).
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("DNS name not found: {0}")]
    NotFound(String),
    #[error("DNS resolution failed: {0}")]
    Transient(String),
}

/// In-memory TXT resolver for tests. Maps a fully-qualified name → TXT strings.
/// Never performs network I/O.
#[derive(Debug, Clone, Default)]
pub struct MockTxtResolver {
    records: HashMap<String, Vec<String>>,
}

impl MockTxtResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a TXT record string under a name. Chainable.
    pub fn with_record(mut self, name: &str, value: &str) -> Self {
        self.records
            .entry(name.to_string())
            .or_default()
            .push(value.to_string());
        self
    }
}

#[async_trait]
impl TxtResolver for MockTxtResolver {
    async fn lookup_txt(&self, name: &str) -> Result<Vec<String>, ResolveError> {
        match self.records.get(name) {
            Some(v) => Ok(v.clone()),
            None => Ok(Vec::new()),
        }
    }
}

/// True if any TXT record at the challenge name carries the expected token,
/// using a constant-time comparison on the token segment (AC-2).
pub fn txt_records_contain_token(records: &[String], expected_token: &str) -> bool {
    let want = format!("{CHALLENGE_VALUE_PREFIX}{expected_token}");
    records.iter().any(|r| {
        let r = r.trim().trim_matches('"');
        constant_time_eq(r, &want)
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Token primitives (mirror anseo-comms posture; no cross-crate dep)
// ─────────────────────────────────────────────────────────────────────────────

/// Generate a high-entropy (≥128-bit) single-use token (AC-1).
///
/// Two v4 UUIDs concatenated give 2×122 = 244 bits of CSPRNG entropy (uuid's
/// v4 generator draws from the OS RNG via `getrandom`), comfortably above the
/// ≥128-bit floor. We hex-encode the 32 raw bytes so the token is URL- and
/// DNS-TXT-safe.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    hex::encode(bytes)
}

/// `sha256(raw)`, lowercase hex — the DB lookup key.
pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

/// Constant-time string equality (avoids token-comparison timing leaks, AC-2).
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    ab.ct_eq(bb).into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Repository
// ─────────────────────────────────────────────────────────────────────────────

pub struct VerificationRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> VerificationRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Count verification attempts for `domain` within the trailing hour. The
    /// API layer rejects a new challenge with 429 once this reaches
    /// [`RATE_LIMIT_PER_HOUR`] (AC-6).
    pub async fn attempts_last_hour(&self, domain: &str) -> Result<i64, Error> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS n
            FROM verification_attempts
            WHERE domain = $1
              AND created_at > now() - INTERVAL '1 hour'
            "#,
        )
        .bind(domain)
        .fetch_one(self.pool)
        .await?;
        Ok(row.get::<i64, _>("n"))
    }

    /// Expire any live (pending, unconsumed) challenge for (domain, method) so a
    /// fresh challenge can be minted without colliding with the live-challenge
    /// unique index. Called before [`Self::create_challenge`].
    pub async fn expire_live_challenges(
        &self,
        domain: &str,
        method: VerificationMethod,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE verification_attempts
            SET status = 'expired'
            WHERE domain = $1
              AND method = $2
              AND status = 'pending'
              AND used_at IS NULL
            "#,
        )
        .bind(domain)
        .bind(method.as_str())
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Mint + store a new challenge token bound to (domain, method) with the
    /// method's TTL and the authorization attestation (AC-1 / AC-7). Returns the
    /// raw token (caller embeds it in the TXT record / magic-link URL).
    ///
    /// The caller MUST have checked the rate limit and expired live challenges
    /// first. `claimant_email` is set for the magic-link method (reuses the
    /// 43.3-owned `claimant_email` column).
    pub async fn create_challenge(
        &self,
        domain: &str,
        method: VerificationMethod,
        claimant_session: Option<&str>,
        claimant_email: Option<&str>,
    ) -> Result<MintedChallenge, Error> {
        let raw = generate_token();
        let token_hash = hash_token(&raw);
        let now = chrono::Utc::now();
        let expires_at = now + method.ttl();
        let id = uuid::Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO verification_attempts
                (id, domain, method, token_hash, claimant_session, claimant_email,
                 status, attestation_version, attested_at, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7, now(), $8)
            "#,
        )
        .bind(id)
        .bind(domain)
        .bind(method.as_str())
        .bind(&token_hash)
        .bind(claimant_session)
        .bind(claimant_email)
        .bind(ATTESTATION_VERSION)
        .bind(expires_at)
        .execute(self.pool)
        .await?;

        Ok(MintedChallenge {
            id,
            raw_token: raw,
            method,
            expires_at,
        })
    }

    /// Look up the live attempt row for a presented raw token. Returns `None`
    /// when the token is unknown. Expiry / consumption are evaluated by the
    /// caller so it can return the precise 401 (AC-4).
    pub async fn find_by_token(&self, raw_token: &str) -> Result<Option<AttemptRecord>, Error> {
        let token_hash = hash_token(raw_token);
        let row = sqlx::query(
            r#"
            SELECT id, domain, method, token_hash,
                   status AS state, expires_at, used_at AS consumed_at
            FROM verification_attempts
            WHERE token_hash = $1
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| AttemptRecord {
            id: r.get("id"),
            domain: r.get("domain"),
            method: r.get("method"),
            token_hash: r.get("token_hash"),
            state: r.get("state"),
            expires_at: r.get("expires_at"),
            consumed_at: r.get("consumed_at"),
        }))
    }

    /// Atomically consume a challenge: stamp `consumed_at` + `state = 'verified'`
    /// IFF the row is still pending, unconsumed, and unexpired. Returns `true`
    /// when this call won the single-use race; `false` means already consumed /
    /// expired (→ 401 replay at the API layer, AC-4).
    pub async fn consume(&self, id: uuid::Uuid) -> Result<bool, Error> {
        let res = sqlx::query(
            r#"
            UPDATE verification_attempts
            SET status = 'verified', used_at = now()
            WHERE id = $1
              AND status = 'pending'
              AND used_at IS NULL
              AND expires_at > now()
            "#,
        )
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(res.rows_affected() == 1)
    }

    /// Mark a challenge failed (e.g. token-mismatch on Check Now). Append-only
    /// ledger semantics: the row records the failed attempt for dispute
    /// evidence.
    pub async fn mark_failed(&self, id: uuid::Uuid) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE verification_attempts
            SET status = 'failed'
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Record a revocation in the ledger (daily job, AC-5). The entity's
    /// claim_status flip + grace period is handled by `EntityRepo`; this writes
    /// the append-only audit row.
    pub async fn record_revocation(&self, domain: &str) -> Result<(), Error> {
        sqlx::query(
            r#"
            INSERT INTO verification_attempts
                (domain, method, token_hash, status, attestation_version, expires_at)
            VALUES ($1, 'dns_txt', '', 'revoked', $2, now())
            "#,
        )
        .bind(domain)
        .bind(ATTESTATION_VERSION)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Domains currently `dns_txt`-verified — the set the daily re-verify job
    /// re-checks (AC-5). Returns `(domain, token_hash)` of the most-recent
    /// consumed dns_txt challenge per domain so the job can confirm the TXT
    /// record is still present.
    pub async fn dns_verified_domains(&self) -> Result<Vec<(String, String)>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT ON (va.domain) va.domain, va.token_hash
            FROM verification_attempts va
            JOIN entities e ON e.domain = va.domain
            WHERE va.method = 'dns_txt'
              AND va.status = 'verified'
              AND e.claim_status = 'verified'
              AND e.verification_method = 'dns_txt'
            ORDER BY va.domain, va.used_at DESC
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("domain"),
                    r.get::<String, _>("token_hash"),
                )
            })
            .collect())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Daily re-verification job (AC-5) — revoke on TXT record removal.
// ─────────────────────────────────────────────────────────────────────────────

/// Re-check every dns_txt-verified domain. When the challenge TXT record is no
/// longer present, transition the entity to `revoked` (starting the grace
/// period) and append a revocation row to the ledger. The registered-email
/// notification uses `anseo-comms` once available; until then we log (per AC-5).
///
/// Lives in `anseo-storage` (not `anseo-api`) so the background **worker** can
/// drive it on a daily cadence without depending on the HTTP crate — it only
/// needs a [`Storage`] handle and a [`TxtResolver`]. The API layer re-exports a
/// thin wrapper. Parameterised on the resolver so production passes the real
/// hickory client and unit tests inject [`MockTxtResolver`] (no network).
///
/// Returns the number of entities revoked this sweep.
pub async fn run_reverification_job(
    storage: &crate::Storage,
    resolver: &dyn TxtResolver,
) -> Result<usize, Error> {
    let verification = storage.verification();
    let entities = storage.entities();
    let domains = verification.dns_verified_domains().await?;
    let mut revoked = 0usize;

    for (domain, token_hash) in domains {
        let record_name = MintedChallenge::dns_record_name(&domain);
        let txt = match resolver.lookup_txt(&record_name).await {
            Ok(v) => v,
            // Transient errors are NOT treated as removal — skip this domain so
            // a flaky resolver never falsely revokes a legitimate verification.
            Err(ResolveError::Transient(_)) => continue,
            Err(ResolveError::NotFound(_)) => Vec::new(),
        };

        // Still present if any resolved TXT value hashes to the stored token.
        let still_present = txt.iter().any(|v| {
            let v = v.trim().trim_matches('"');
            v.strip_prefix("anseo-verify=")
                .map(|raw| hash_token(raw) == token_hash)
                .unwrap_or(false)
        });

        if !still_present {
            entities.set_grace_period_start(&domain).await?;
            verification.record_revocation(&domain).await?;
            revoked += 1;
            // AC-5: notify the registered email via anseo-comms once wired.
            tracing::warn!(
                event = "verification.revoked",
                domain = %domain,
                "DNS-TXT record removed; entity revoked + grace period started. \
                 Revocation notification pending comms wire."
            );
        }
    }
    Ok(revoked)
}

/// Pure helper: classify why a presented token is invalid, given the stored
/// row (or absence). Returns `Ok(())` when the token may be consumed, else the
/// reason. Extracted so it is unit-testable without a DB (AC-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenRejection {
    Unknown,
    AlreadyConsumed,
    Expired,
    WrongState,
}

/// Validate a presented token against its stored row at `now`. `record` is the
/// row found by [`VerificationRepo::find_by_token`] (or `None` when unknown).
pub fn classify_token(
    record: Option<&AttemptRecord>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), TokenRejection> {
    let r = match record {
        Some(r) => r,
        None => return Err(TokenRejection::Unknown),
    };
    if r.consumed_at.is_some() {
        return Err(TokenRejection::AlreadyConsumed);
    }
    if r.expires_at <= now {
        return Err(TokenRejection::Expired);
    }
    if r.state != "pending" {
        return Err(TokenRejection::WrongState);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_high_entropy_and_hex() {
        let t = generate_token();
        assert_eq!(t.len(), 64, "32 bytes = 256 bits = 64 hex chars (≥128-bit)");
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        // Two mints differ (overwhelmingly).
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn hash_of_raw_is_stable_and_distinct() {
        let t = generate_token();
        assert_eq!(hash_token(&t), hash_token(&t));
        assert_ne!(hash_token(&t), hash_token(&generate_token()));
        assert_eq!(hash_token(&t).len(), 64);
    }

    #[test]
    fn constant_time_eq_matches_and_rejects() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "abcd")); // length mismatch
    }

    #[test]
    fn dns_record_name_and_value_shape() {
        let name = MintedChallenge::dns_record_name("example.com");
        assert_eq!(name, "_anseo-challenge.example.com");
        let ch = MintedChallenge {
            id: uuid::Uuid::nil(),
            raw_token: "deadbeef".into(),
            method: VerificationMethod::DnsTxt,
            expires_at: chrono::Utc::now(),
        };
        assert_eq!(ch.dns_record_value(), "anseo-verify=deadbeef");
    }

    #[test]
    fn txt_match_handles_quotes_and_whitespace() {
        let token = "abc123";
        let want = format!("anseo-verify={token}");
        assert!(txt_records_contain_token(
            std::slice::from_ref(&want),
            token
        ));
        // Quoted form as some resolvers return it.
        assert!(txt_records_contain_token(&[format!("\"{want}\"")], token));
        // Trailing whitespace tolerated.
        assert!(txt_records_contain_token(&[format!("  {want}  ")], token));
        // Wrong token rejected.
        assert!(!txt_records_contain_token(&[want], "nope"));
        // No records → no match.
        assert!(!txt_records_contain_token(&[], token));
    }

    #[tokio::test]
    async fn mock_resolver_returns_configured_and_empty() {
        let r =
            MockTxtResolver::new().with_record("_anseo-challenge.example.com", "anseo-verify=tok");
        assert_eq!(
            r.lookup_txt("_anseo-challenge.example.com").await.unwrap(),
            vec!["anseo-verify=tok".to_string()]
        );
        // Unknown name → empty (drives revocation, not an error).
        assert!(r
            .lookup_txt("_anseo-challenge.absent.com")
            .await
            .unwrap()
            .is_empty());
    }

    fn rec(consumed: bool, expired: bool, state: &str) -> AttemptRecord {
        let now = chrono::Utc::now();
        AttemptRecord {
            id: uuid::Uuid::nil(),
            domain: "example.com".into(),
            method: "dns_txt".into(),
            token_hash: "h".into(),
            state: state.into(),
            expires_at: if expired {
                now - chrono::Duration::minutes(1)
            } else {
                now + chrono::Duration::hours(1)
            },
            consumed_at: consumed.then_some(now),
        }
    }

    #[test]
    fn classify_token_covers_all_rejections() {
        let now = chrono::Utc::now();
        // Unknown token.
        assert_eq!(classify_token(None, now), Err(TokenRejection::Unknown));
        // Already consumed (replay → 401, AC-4).
        assert_eq!(
            classify_token(Some(&rec(true, false, "verified")), now),
            Err(TokenRejection::AlreadyConsumed)
        );
        // Expired (>TTL → 401, AC-4).
        assert_eq!(
            classify_token(Some(&rec(false, true, "pending")), now),
            Err(TokenRejection::Expired)
        );
        // Non-pending state.
        assert_eq!(
            classify_token(Some(&rec(false, false, "failed")), now),
            Err(TokenRejection::WrongState)
        );
        // Valid → consumable.
        assert_eq!(
            classify_token(Some(&rec(false, false, "pending")), now),
            Ok(())
        );
    }

    #[test]
    fn method_ttl_and_ranking_policy() {
        assert_eq!(
            VerificationMethod::DnsTxt.ttl(),
            chrono::Duration::hours(48)
        );
        assert_eq!(
            VerificationMethod::EmailMagicLink.ttl(),
            chrono::Duration::minutes(30)
        );
        // NFR8: only DNS-TXT qualifies for ranked placement.
        assert!(VerificationMethod::DnsTxt.qualifies_for_ranking());
        assert!(!VerificationMethod::EmailMagicLink.qualifies_for_ranking());
    }
}
