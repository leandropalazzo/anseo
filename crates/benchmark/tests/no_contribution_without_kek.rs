//! Story 39.1 — runtime hard-gate: no contribution may be sealed without a
//! per-project KEK.
//!
//! These integration tests verify the complete gate end-to-end:
//!
//! 1. `ProjectKek::load` returns `CryptoError::KekMissing` when no KEK is
//!    present — the caller cannot obtain the value needed by `Redactor` or
//!    `ProjectKek::seal`.
//! 2. After `ProjectKek::destroy` the gate trips again even though the
//!    project previously had a KEK.
//! 3. The `ContributeIngest` trait signature requires `&ProjectKek`, so a
//!    conforming implementation cannot be called without one (the compile-time
//!    half of the gate is tested implicitly by the trait bound in the
//!    [`DummyIngest`] implementation below).

use anseo_benchmark::{
    ContributeIngest, CryptoError, ProjectKek, RawPromptRun, Redactor, SealedContribution,
    TERMS_VERSION,
};
use anseo_core::InMemoryStore;
use chrono::{TimeZone, Utc};

const PROJECT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn raw_run() -> RawPromptRun {
    RawPromptRun {
        project_id: PROJECT.into(),
        prompt_slug: "vector-db".into(),
        provider: "openai".into(),
        model: "gpt-4o-2024-08-06".into(),
        observed_at: Utc.with_ymd_and_hms(2026, 6, 15, 8, 0, 0).unwrap(),
        observed_rank: Some(1),
        citation_domains: vec!["example.com".into()],
        brand_name: "ACME".into(),
        raw_response_text: "some response".into(),
        api_key_used: "sk-test".into(),
        ip_address: "127.0.0.1".into(),
    }
}

/// A minimal `ContributeIngest` implementation that uses only OSS types.
/// This tests the Epic 40 seam boundary: the trait requires `&ProjectKek`,
/// making it compile-impossible to call without one.
struct DummyIngest;

#[derive(Debug)]
enum DummyError {
    Redactor(anseo_benchmark::RedactorError),
    Crypto(CryptoError),
}

impl std::fmt::Display for DummyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DummyError::Redactor(e) => write!(f, "redactor: {e}"),
            DummyError::Crypto(e) => write!(f, "crypto: {e}"),
        }
    }
}

impl std::error::Error for DummyError {}

impl ContributeIngest for DummyIngest {
    type Error = DummyError;

    fn contribute(
        &self,
        kek: &ProjectKek,
        run: RawPromptRun,
        consented_terms: &str,
    ) -> Result<SealedContribution, Self::Error> {
        let payload = Redactor::new(kek, consented_terms)
            .redact(run)
            .map_err(DummyError::Redactor)?;
        kek.seal(&payload).map_err(DummyError::Crypto)
    }
}

// ── Gate tests ────────────────────────────────────────────────────────────────

#[test]
fn load_without_provisioned_kek_returns_kek_missing() {
    let store = InMemoryStore::durable_for_tests();
    let err = ProjectKek::load(&store, PROJECT).unwrap_err();
    assert!(
        matches!(err, CryptoError::KekMissing { .. }),
        "expected KekMissing, got {err:?}"
    );
}

#[test]
fn contribute_ingest_requires_kek_type_gate() {
    // The DummyIngest::contribute signature demands &ProjectKek.
    // This test proves the gate by using a properly provisioned KEK.
    let store = InMemoryStore::durable_for_tests();
    let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
    let ingest = DummyIngest;
    let result = ingest.contribute(&kek, raw_run(), TERMS_VERSION);
    assert!(
        result.is_ok(),
        "expected Ok sealed contribution, got {result:?}"
    );
}

#[test]
fn after_destroy_load_returns_kek_missing_and_contribution_is_impossible() {
    let store = InMemoryStore::durable_for_tests();
    // Provision, seal one contribution, then destroy.
    let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
    let ingest = DummyIngest;
    assert!(ingest.contribute(&kek, raw_run(), TERMS_VERSION).is_ok());

    ProjectKek::destroy(&store, PROJECT).unwrap();

    // Runtime gate: load now fails.
    let err = ProjectKek::load(&store, PROJECT).unwrap_err();
    assert!(
        matches!(err, CryptoError::KekMissing { .. }),
        "expected KekMissing after destroy, got {err:?}"
    );
    // Without a KEK value in hand, ContributeIngest::contribute cannot be
    // called — the type system enforces this at compile time.
}

#[test]
fn ephemeral_store_refuses_kek_provisioning() {
    // An ephemeral-only store has no durable backend; provisioning a KEK
    // that would vanish on restart must be refused, leaving the gate closed.
    let ephemeral = InMemoryStore::new();
    let err = ProjectKek::load_or_create(&ephemeral, PROJECT).unwrap_err();
    assert!(
        matches!(err, CryptoError::EphemeralKek { .. }),
        "expected EphemeralKek, got {err:?}"
    );
    // Confirm nothing was written — gate still closed.
    let load_err = ProjectKek::load(&ephemeral, PROJECT).unwrap_err();
    assert!(
        matches!(load_err, CryptoError::KekMissing { .. }),
        "expected KekMissing after refused provisioning, got {load_err:?}"
    );
}
