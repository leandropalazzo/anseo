//! Phase 2 Story 13.1 — public benchmark dataset contribution boundary.
//!
//! Architecture §2.3 / §7: opt-in contributions transmit a redacted
//! subset of each Prompt Run. The privacy guarantee is **compile-time**:
//! [`BenchmarkPayload`] has only private fields and no public
//! constructor, so the ONLY way to materialize one is via
//! [`Redactor::redact`]. A caller cannot accidentally hand-craft a
//! payload with non-redacted fields. The [`compile_fail`] doctests
//! below prove the constructor is unreachable from outside this crate.
//!
//! The Phase 2 benchmark **service** (the ingest endpoint that receives
//! these payloads) lives in a separate repo (`benchmark-service`) per
//! architecture §7. This crate ships only the OSS client surface:
//! redaction, HMAC project identity, and the contribution-terms version
//! pinning the legal text the operator consented to.
//!
//! # Story 39.1 hard gate
//!
//! As of Story 39.1, a [`ProjectKek`] is **required** before any contribution
//! can be produced. There is no fallback to a global master secret. The
//! [`ContributeIngest`] trait (Epic 40 seam) encodes this requirement in its
//! type signature: `fn contribute` takes `&ProjectKek`, making it impossible
//! to call without one.

pub mod canonical_suite;
pub mod crypto;
pub mod redactor;

pub use canonical_suite::{
    canonical_geo_prompt_suite, canonical_prompt_by_slug, CanonicalPromptEntry,
    CanonicalPromptSuite, SuiteOwnership,
};
pub use crypto::{kek_secret_key, CryptoError, ProjectKek, SealedContribution};
pub use redactor::{
    BenchmarkPayload, ProjectHmac, RawPromptRun, Redactor, RedactorError, TERMS_VERSION,
};

/// Epic 40 ingest seam.
///
/// Any implementation that ingests a [`RawPromptRun`] into the benchmark
/// dataset **must** hold a [`ProjectKek`]. This is the typed boundary that
/// prevents contributions from bypassing the per-project key gate.
///
/// The blanket wire implementation (HTTP POST to the benchmark service) will
/// live in `opengeo-internal`; the OSS crate ships only this trait so
/// upstream code can program against the interface without depending on the
/// closed-source HTTP layer.
pub trait ContributeIngest {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Seal `run` against `kek` and forward the [`SealedContribution`] to the
    /// benchmark ingest endpoint. Returns a typed error if the KEK is absent,
    /// the redaction rejects the payload, or the transport fails.
    ///
    /// Callers must hold a live `&ProjectKek` — the type system prevents
    /// calling this function without one, which is the compile-time half of
    /// the Story 39.1 contribution gate.
    fn contribute(
        &self,
        kek: &ProjectKek,
        run: RawPromptRun,
        consented_terms: &str,
    ) -> Result<SealedContribution, Self::Error>;
}
