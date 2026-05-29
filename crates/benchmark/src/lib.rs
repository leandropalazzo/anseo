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

pub mod redactor;

pub use redactor::{
    BenchmarkPayload, ProjectHmac, RawPromptRun, Redactor, RedactorError, TERMS_VERSION,
};
