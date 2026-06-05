//! Compile-time redaction boundary for the public benchmark dataset.
//!
//! [`BenchmarkPayload`] is the only struct shape that crosses to the
//! benchmark service. Its fields are private and there is **no** public
//! constructor — the only way to materialize one is via
//! [`Redactor::redact`], which projects from the rich [`RawPromptRun`]
//! shape to the narrow public shape, dropping fields that must never
//! transmit (brand name, free-form text, IP, secrets, etc.).
//!
//! Compile-time proof that the constructor is unreachable from outside
//! this crate:
//!
//! ```compile_fail
//! use anseo_benchmark::BenchmarkPayload;
//! // The struct has no public fields, so this never compiles.
//! let _ = BenchmarkPayload { observed_rank: Some(2) };
//! ```
//!
//! ```compile_fail
//! use anseo_benchmark::BenchmarkPayload;
//! // There's no `new` or `From` constructor exposed either.
//! let _ = BenchmarkPayload::new();
//! ```

use chrono::{DateTime, Timelike, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::crypto::ProjectKek;

type HmacSha256 = Hmac<Sha256>;

/// Pin the legal-text version the operator most recently consented to.
/// The CLI's `optin` flow writes this to a `consent` row; the redactor
/// refuses to produce a payload if the operator's pinned version is
/// behind. A version bump triggers a re-consent dialog.
pub const TERMS_VERSION: &str = "v1-2026-05-28";

/// HMAC identifying a project across contributions without revealing the
/// project's brand name. Computed as
/// `hex(HMAC-SHA256(secret = project_kek_bytes, msg = project_id))`.
///
/// As of Story 39.1 this is a **linkage-only** identifier: it lets the
/// benchmark service group a project's contributions together, but it is no
/// longer the erasure mechanism. Erasure is achieved by destroying the
/// project's KEK (see [`crate::crypto::ProjectKek`]), which renders every
/// sealed contribution undecryptable. The HMAC key is derived from the KEK
/// bytes, so destroying the KEK also retires the linkage key.
///
/// **Security hard gate**: there is NO `compute(raw_bytes, …)` constructor.
/// The only way to produce a `ProjectHmac` is via
/// [`ProjectHmac::from_kek`], which requires a live [`ProjectKek`]. This is
/// the compile-time half of the Story 39.1 gate: no KEK → no linkage
/// identifier → no `BenchmarkPayload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectHmac(String);

impl ProjectHmac {
    /// Derive the linkage HMAC from `kek`'s internal key bytes.
    ///
    /// The KEK bytes double as the HMAC key, so destroying the KEK also
    /// retires the linkage identifier for every sealed contribution.
    pub fn from_kek(kek: &ProjectKek, project_id: &str) -> Self {
        let mut mac = HmacSha256::new_from_slice(kek.linkage_key())
            .expect("HMAC-SHA256 accepts any key length");
        mac.update(project_id.as_bytes());
        let bytes = mac.finalize().into_bytes();
        let mut hex = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            hex.push_str(&format!("{b:02x}"));
        }
        Self(hex)
    }

    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

/// What the caller knows about one Prompt Run, before redaction. This is
/// the public, in-crate shape — wider than `BenchmarkPayload` because
/// it carries everything we might want to look at for redaction decisions.
/// Crucially, **constructing one of these is fine**; nothing here is
/// confidential. The privacy boundary is between this and
/// `BenchmarkPayload`.
#[derive(Debug, Clone, PartialEq)]
pub struct RawPromptRun {
    pub project_id: String,
    pub prompt_slug: String,
    pub provider: String,
    pub model: String,
    pub observed_at: DateTime<Utc>,
    pub observed_rank: Option<i32>,
    /// Source-domains observed in citations. The caller deduplicates
    /// before passing in; the redactor doesn't re-validate.
    pub citation_domains: Vec<String>,
    // Fields that look like they'd leak if passed through. The redactor
    // intentionally ignores them.
    pub brand_name: String,
    pub raw_response_text: String,
    pub api_key_used: String,
    pub ip_address: String,
}

/// The redacted shape that crosses to the benchmark service.
///
/// All fields are PRIVATE. The struct has no public constructor. The
/// only way to produce one is via [`Redactor::redact`]. Cross-crate
/// hand-construction of a payload with non-redacted content is
/// **compile-impossible** — see the doctests in this module.
///
/// Serializes via serde as the wire JSON shape the benchmark service
/// accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkPayload {
    prompt_slug: String,
    provider: String,
    model: String,
    /// UTC, rounded to the hour for k-anonymity (an observation at
    /// 08:43:21 reports as 08:00:00).
    observed_at_hour: DateTime<Utc>,
    observed_rank: Option<i32>,
    citation_domains: Vec<String>,
    project_hmac: ProjectHmac,
    terms_version: String,
}

impl BenchmarkPayload {
    // Intentionally NO `pub fn new(...)`. The only constructor is
    // `Redactor::redact`. Public accessors below let downstream code
    // read fields without breaking the construction boundary.

    pub fn prompt_slug(&self) -> &str {
        &self.prompt_slug
    }
    pub fn provider(&self) -> &str {
        &self.provider
    }
    pub fn model(&self) -> &str {
        &self.model
    }
    pub fn observed_at_hour(&self) -> DateTime<Utc> {
        self.observed_at_hour
    }
    pub fn observed_rank(&self) -> Option<i32> {
        self.observed_rank
    }
    pub fn citation_domains(&self) -> &[String] {
        &self.citation_domains
    }
    pub fn project_hmac(&self) -> &ProjectHmac {
        &self.project_hmac
    }
    pub fn terms_version(&self) -> &str {
        &self.terms_version
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RedactorError {
    #[error(
        "operator consented to terms version `{consented}` but the current \
         terms are `{TERMS_VERSION}`; ask them to run `ogeo benchmark optin` \
         to re-consent before contributing"
    )]
    StaleConsent { consented: String },
    #[error("prompt slug `{slug}` is not slug-safe (must be lowercase ASCII + digits + hyphens)")]
    InvalidSlug { slug: String },
}

/// The redaction projection.
///
/// As of Story 39.1 the redactor is parameterized by a [`ProjectKek`] rather
/// than a raw global master secret. Holding a `&ProjectKek` is the type-level
/// half of the hard gate: a `BenchmarkPayload` can only be produced when a
/// per-project KEK has already been loaded from the secret store. The KEK
/// bytes double as the HMAC linkage key (see [`ProjectHmac`]).
///
/// The operator's pinned terms version is carried alongside so each `redact`
/// call can refuse stale consent without re-fetching.
pub struct Redactor<'a> {
    kek: &'a ProjectKek,
    consented_terms: &'a str,
}

impl<'a> Redactor<'a> {
    pub fn new(kek: &'a ProjectKek, consented_terms: &'a str) -> Self {
        Self {
            kek,
            consented_terms,
        }
    }

    /// Project from `RawPromptRun` to `BenchmarkPayload`. The dropped
    /// fields are: `brand_name`, `raw_response_text`, `api_key_used`,
    /// `ip_address`. They never reach the returned struct, so they
    /// can never reach the wire.
    pub fn redact(&self, raw: RawPromptRun) -> Result<BenchmarkPayload, RedactorError> {
        if self.consented_terms != TERMS_VERSION {
            return Err(RedactorError::StaleConsent {
                consented: self.consented_terms.to_string(),
            });
        }
        if !is_slug_safe(&raw.prompt_slug) {
            return Err(RedactorError::InvalidSlug {
                slug: raw.prompt_slug,
            });
        }

        let project_hmac = ProjectHmac::from_kek(self.kek, &raw.project_id);
        let observed_at_hour = round_to_hour(raw.observed_at);

        Ok(BenchmarkPayload {
            prompt_slug: raw.prompt_slug,
            provider: raw.provider,
            model: raw.model,
            observed_at_hour,
            observed_rank: raw.observed_rank,
            citation_domains: raw.citation_domains,
            project_hmac,
            terms_version: TERMS_VERSION.to_string(),
        })
    }
}

fn is_slug_safe(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn round_to_hour(ts: DateTime<Utc>) -> DateTime<Utc> {
    // Defensive fallback: chrono's with_* return None only on overflow,
    // which shouldn't happen for any realistic input — but if it did,
    // returning the original ts is preferable to a panic.
    ts.with_minute(0)
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::ProjectKek;
    use anseo_core::InMemoryStore;
    use chrono::{Datelike, TimeZone};

    /// A KEK to drive `Redactor` in tests. Provisioned in a throwaway
    /// in-memory store so each test gets a real per-project key.
    fn test_kek() -> ProjectKek {
        // `load_or_create` is durable-or-fail; use the durable test store so a
        // real per-project KEK is provisioned without a keyring/age-file.
        let store = InMemoryStore::durable_for_tests();
        ProjectKek::load_or_create(&store, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    fn raw_fixture() -> RawPromptRun {
        RawPromptRun {
            project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
            prompt_slug: "vector-db".into(),
            provider: "openai".into(),
            model: "gpt-4o-2024-08-06".into(),
            observed_at: Utc.with_ymd_and_hms(2026, 6, 15, 8, 43, 21).unwrap(),
            observed_rank: Some(2),
            citation_domains: vec!["docs.example.com".into(), "wikipedia.org".into()],
            brand_name: "Pinecone".into(),
            raw_response_text: "Pinecone is a leading vector database…".into(),
            api_key_used: "sk-secret".into(),
            ip_address: "10.0.0.1".into(),
        }
    }

    #[test]
    fn redact_produces_payload_with_expected_fields() {
        let kek = test_kek();
        let r = Redactor::new(&kek, TERMS_VERSION);
        let payload = r.redact(raw_fixture()).unwrap();
        assert_eq!(payload.prompt_slug(), "vector-db");
        assert_eq!(payload.provider(), "openai");
        assert_eq!(payload.observed_rank(), Some(2));
    }

    #[test]
    fn redact_drops_brand_name_raw_response_apikey_ip() {
        let kek = test_kek();
        let r = Redactor::new(&kek, TERMS_VERSION);
        let payload = r.redact(raw_fixture()).unwrap();
        let json = serde_json::to_string(&payload).unwrap();
        // These confidential fields must not appear in the serialized
        // payload anywhere.
        assert!(!json.contains("Pinecone"), "brand_name leaked: {json}");
        assert!(
            !json.contains("vector database"),
            "raw_response leaked: {json}"
        );
        assert!(!json.contains("sk-secret"), "api_key_used leaked: {json}");
        assert!(!json.contains("10.0.0.1"), "ip_address leaked: {json}");
    }

    #[test]
    fn redact_rounds_timestamp_to_hour_for_k_anonymity() {
        let kek = test_kek();
        let r = Redactor::new(&kek, TERMS_VERSION);
        let payload = r.redact(raw_fixture()).unwrap();
        assert_eq!(payload.observed_at_hour().minute(), 0);
        assert_eq!(payload.observed_at_hour().second(), 0);
        assert_eq!(payload.observed_at_hour().hour(), 8);
    }

    #[test]
    fn redact_refuses_stale_consent() {
        let kek = test_kek();
        let r = Redactor::new(&kek, "v0-old-version");
        let err = r.redact(raw_fixture()).unwrap_err();
        match err {
            RedactorError::StaleConsent { consented } => {
                assert_eq!(consented, "v0-old-version");
            }
            other => panic!("expected StaleConsent, got {other:?}"),
        }
    }

    #[test]
    fn redact_refuses_invalid_slug() {
        let mut raw = raw_fixture();
        raw.prompt_slug = "Has Caps and Spaces!".into();
        let kek = test_kek();
        let r = Redactor::new(&kek, TERMS_VERSION);
        assert!(matches!(
            r.redact(raw),
            Err(RedactorError::InvalidSlug { .. })
        ));
    }

    #[test]
    fn project_hmac_is_stable_across_invocations() {
        // Same KEK + same project_id → identical linkage HMAC every time.
        let kek = test_kek();
        let h1 = ProjectHmac::from_kek(&kek, "01ARZ");
        let h2 = ProjectHmac::from_kek(&kek, "01ARZ");
        assert_eq!(h1, h2);
        assert_eq!(h1.as_hex().len(), 64);
    }

    #[test]
    fn project_hmac_changes_with_kek_rotation() {
        // Story 39.1 KEK-rotation replaces the old master-secret-rotation
        // test. A fresh KEK for the same project yields a different linkage
        // HMAC, which is the expected behaviour after a KEK is destroyed and
        // re-provisioned.
        let store = InMemoryStore::durable_for_tests();
        let kek1 = ProjectKek::load_or_create(&store, "01ARZ").unwrap();
        let h1 = ProjectHmac::from_kek(&kek1, "01ARZ");
        // Destroy the first KEK, then provision a fresh one.
        ProjectKek::destroy(&store, "01ARZ").unwrap();
        let kek2 = ProjectKek::load_or_create(&store, "01ARZ").unwrap();
        let h2 = ProjectHmac::from_kek(&kek2, "01ARZ");
        assert_ne!(h1, h2, "rotated KEK must yield a different linkage HMAC");
    }

    #[test]
    fn project_hmac_differs_across_project_ids() {
        // Same KEK, different project_id → different HMAC.
        let kek = test_kek();
        let h1 = ProjectHmac::from_kek(&kek, "01ARZ");
        let h2 = ProjectHmac::from_kek(&kek, "01XYZ");
        assert_ne!(h1, h2);
    }

    #[test]
    fn payload_serializes_with_expected_field_names() {
        // Wire shape pin: the benchmark service deserializes these names.
        let kek = test_kek();
        let r = Redactor::new(&kek, TERMS_VERSION);
        let payload = r.redact(raw_fixture()).unwrap();
        let json = serde_json::to_value(&payload).unwrap();
        for expected_field in [
            "prompt_slug",
            "provider",
            "model",
            "observed_at_hour",
            "observed_rank",
            "citation_domains",
            "project_hmac",
            "terms_version",
        ] {
            assert!(
                json.get(expected_field).is_some(),
                "wire field `{expected_field}` missing from payload"
            );
        }
    }

    #[test]
    fn payload_round_trips_through_serde() {
        let kek = test_kek();
        let r = Redactor::new(&kek, TERMS_VERSION);
        let payload = r.redact(raw_fixture()).unwrap();
        let bytes = serde_json::to_vec(&payload).unwrap();
        let back: BenchmarkPayload = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, payload);
    }

    #[test]
    fn payload_carries_terms_version_pinning() {
        let kek = test_kek();
        let r = Redactor::new(&kek, TERMS_VERSION);
        let payload = r.redact(raw_fixture()).unwrap();
        assert_eq!(payload.terms_version(), TERMS_VERSION);
    }

    #[test]
    fn slug_safe_predicate_pins_phase1_rule() {
        assert!(is_slug_safe("vector-db"));
        assert!(is_slug_safe("a"));
        assert!(is_slug_safe("123"));
        assert!(!is_slug_safe(""));
        assert!(!is_slug_safe("ABC"));
        assert!(!is_slug_safe("has space"));
        assert!(!is_slug_safe("under_score"));
    }

    #[test]
    fn round_to_hour_drops_minutes_seconds_nanos() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 15, 8, 43, 21).unwrap();
        let rounded = round_to_hour(ts);
        assert_eq!(rounded.minute(), 0);
        assert_eq!(rounded.second(), 0);
        assert_eq!(rounded.hour(), 8);
        assert_eq!(rounded.day(), 15);
    }
}
