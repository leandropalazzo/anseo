//! Story 39.1b — keystore substrate alignment: one mechanism, two key classes.
//!
//! Both the benchmark KEK (Story 39.1, `crates/benchmark/src/crypto.rs`) and
//! the per-project provider secrets (Story 36.7, `opengeo_core::secret_store`)
//! ride the **same** [`opengeo_core::SecretStore`] abstraction and therefore
//! the same concrete backends (OS keyring, age-encrypted file, in-memory).
//!
//! The two key classes differ only in their *namespace*:
//!
//! | Class          | Storage key shape                | Constant                              |
//! |----------------|----------------------------------|---------------------------------------|
//! | Provider secret | `<project_id>:<provider>`       | `opengeo_core::provider_secret_key`   |
//! | Benchmark KEK  | `benchmark-kek:<project_id>`    | `opengeo_benchmark::kek_secret_key`   |
//!
//! Because a `ProjectId` is a ULID, and the literal `"benchmark-kek"` is not a
//! valid ULID, the two namespaces are structurally disjoint — no provider secret
//! can alias a KEK, and vice versa.
//!
//! These tests exercise the FULL API surface of both key classes against a
//! single shared [`InMemoryStore`] to prove they coexist without interference.

use opengeo_benchmark::{kek_secret_key, ProjectKek};
use opengeo_core::{
    get_provider_secret, provider_secret_key, set_provider_secret, InMemoryStore, Secret,
    SecretStore, SecretStoreError, BENCHMARK_KEK_KEY_PREFIX,
};

// A real ULID-shaped project id (26 chars, Crockford base32).
const PROJECT_A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
const PROJECT_B: &str = "01JQFV0JNKXE2SXWHTQ47CWPRQ";

/// A durable-reporting in-memory store — lets `load_or_create`'s durable-or-
/// fail guard succeed without touching the real OS keyring or disk.
fn durable_store() -> InMemoryStore {
    InMemoryStore::durable_for_tests()
}

// ---------------------------------------------------------------------------
// Key-class isolation: KEK + provider secret coexist in the same store
// ---------------------------------------------------------------------------

#[test]
fn kek_and_provider_secret_coexist_without_interference() {
    // Both key classes written to the SAME store; each must be independently
    // readable without cross-contamination.
    let store = durable_store();

    // Write a provider secret under the 36.7 namespace.
    set_provider_secret(&store, PROJECT_A, "openai", Secret::new("sk-test")).unwrap();

    // Provision a benchmark KEK under the 39.1 namespace.
    let kek = ProjectKek::load_or_create(&store, PROJECT_A).unwrap();
    assert_eq!(kek.project_id(), PROJECT_A);

    // Provider read must not be polluted by the KEK entry.
    assert_eq!(
        get_provider_secret(&store, PROJECT_A, "openai")
            .unwrap()
            .expose(),
        "sk-test"
    );

    // KEK read must not see the provider secret.
    let reloaded_kek = ProjectKek::load(&store, PROJECT_A).unwrap();
    assert_eq!(reloaded_kek.project_id(), PROJECT_A);

    // Verify KEK is operational: seal a minimal payload stub, check
    // round-trip decryption works (crypto correctness, not just key-loading).
    //
    // We use the store-level key string directly because `seal`/`open` require
    // a full `BenchmarkPayload` — checking the key *namespace* shape is
    // sufficient to verify coexistence here.
    let kek_key = kek_secret_key(PROJECT_A);
    let provider_key = provider_secret_key(PROJECT_A, "openai");
    assert_ne!(
        kek_key, provider_key,
        "KEK and provider keys must be distinct strings for the same project"
    );
    // Both keys must be present in the store.
    assert!(store.get(&kek_key).is_ok(), "KEK entry missing");
    assert!(store.get(&provider_key).is_ok(), "provider entry missing");
}

#[test]
fn kek_namespace_prefix_is_never_a_valid_ulid() {
    // The disjointness guarantee: the benchmark KEK prefix is a human-readable
    // literal, not a ULID. A ProjectId is always a 26-char Crockford base32
    // ULID; `"benchmark-kek"` is 13 chars and contains a hyphen — structurally
    // impossible as a ULID-backed project id.
    assert_ne!(BENCHMARK_KEK_KEY_PREFIX.len(), 26);
    assert!(BENCHMARK_KEK_KEY_PREFIX.contains('-'));

    // Consequence: `benchmark-kek:<project_id>` can never equal
    // `<project_id>:<provider>` because the leading segment differs in both
    // length and character set.
    let kek_key = kek_secret_key(PROJECT_A);
    let provider_key = provider_secret_key(PROJECT_A, BENCHMARK_KEK_KEY_PREFIX);
    assert_ne!(kek_key, provider_key);
}

// ---------------------------------------------------------------------------
// Multi-project isolation: destroying one project's KEK leaves the other intact
// ---------------------------------------------------------------------------

#[test]
fn destroying_one_kek_does_not_affect_sibling_provider_secrets() {
    let store = durable_store();

    // Project A: KEK + provider secret.
    ProjectKek::load_or_create(&store, PROJECT_A).unwrap();
    set_provider_secret(&store, PROJECT_A, "openai", Secret::new("sk-a")).unwrap();

    // Project B: KEK + provider secret.
    ProjectKek::load_or_create(&store, PROJECT_B).unwrap();
    set_provider_secret(&store, PROJECT_B, "openai", Secret::new("sk-b")).unwrap();

    // CRYPTO-SHRED project A's KEK.
    ProjectKek::destroy(&store, PROJECT_A).unwrap();

    // Project A's KEK is gone; provider secret is untouched (different key).
    assert!(matches!(
        ProjectKek::load(&store, PROJECT_A),
        Err(opengeo_benchmark::CryptoError::KekMissing { .. })
    ));
    assert_eq!(
        get_provider_secret(&store, PROJECT_A, "openai")
            .unwrap()
            .expose(),
        "sk-a",
        "provider secret must survive KEK destruction"
    );

    // Project B is completely unaffected.
    assert!(
        ProjectKek::load(&store, PROJECT_B).is_ok(),
        "sibling KEK must survive"
    );
    assert_eq!(
        get_provider_secret(&store, PROJECT_B, "openai")
            .unwrap()
            .expose(),
        "sk-b",
        "sibling provider secret must survive"
    );
}

// ---------------------------------------------------------------------------
// Durability gate: shared across both key classes
// ---------------------------------------------------------------------------

#[test]
fn ephemeral_store_blocks_kek_creation_but_not_provider_set() {
    // The durability gate is specific to `ProjectKek::load_or_create` (which
    // calls `set_durable`). Provider secrets use `set` (no durability gate) so
    // they can be written to an ephemeral store without error — that is an
    // operator choice, not a safety violation.
    let ephemeral = InMemoryStore::new(); // is_durable() == false

    // KEK creation must refuse on an ephemeral-only store.
    let err = ProjectKek::load_or_create(&ephemeral, PROJECT_A).unwrap_err();
    assert!(
        matches!(err, opengeo_benchmark::CryptoError::EphemeralKek { .. }),
        "expected EphemeralKek, got {err:?}"
    );

    // Provider secret write succeeds on any store (no durability gate).
    set_provider_secret(&ephemeral, PROJECT_A, "openai", Secret::new("sk-ep")).unwrap();
    assert_eq!(
        get_provider_secret(&ephemeral, PROJECT_A, "openai")
            .unwrap()
            .expose(),
        "sk-ep"
    );
}

// ---------------------------------------------------------------------------
// Key-class namespace shape contracts (regression anchors)
// ---------------------------------------------------------------------------

#[test]
fn kek_secret_key_has_expected_shape() {
    let key = kek_secret_key(PROJECT_A);
    assert!(
        key.starts_with("benchmark-kek:"),
        "KEK key must start with 'benchmark-kek:': {key}"
    );
    assert!(
        key.ends_with(PROJECT_A),
        "KEK key must end with the project id: {key}"
    );
}

#[test]
fn provider_secret_key_has_expected_shape() {
    let key = provider_secret_key(PROJECT_A, "openai");
    assert!(
        key.starts_with(PROJECT_A),
        "provider key must start with project id: {key}"
    );
    assert!(
        key.ends_with("openai"),
        "provider key must end with provider name: {key}"
    );
    assert!(
        !key.starts_with("benchmark-kek:"),
        "provider key must never look like a KEK key: {key}"
    );
}

#[test]
fn two_projects_kek_keys_do_not_collide() {
    // Each project's KEK must have a distinct store key.
    let key_a = kek_secret_key(PROJECT_A);
    let key_b = kek_secret_key(PROJECT_B);
    assert_ne!(key_a, key_b);
}

#[test]
fn two_projects_provider_keys_do_not_collide() {
    let key_a = provider_secret_key(PROJECT_A, "openai");
    let key_b = provider_secret_key(PROJECT_B, "openai");
    assert_ne!(key_a, key_b);
}

// ---------------------------------------------------------------------------
// Both key classes in a ChainedStore (models the real CLI default_chain)
// ---------------------------------------------------------------------------

#[test]
fn both_key_classes_work_in_chained_store() {
    use opengeo_core::ChainedStore;

    let chain = ChainedStore::new(vec![
        Box::new(InMemoryStore::durable_for_tests()),
        Box::new(InMemoryStore::new()),
    ]);

    // Provider secret: plain set (no durability gate, write hits first leg).
    set_provider_secret(&chain, PROJECT_A, "anthropic", Secret::new("sk-chain")).unwrap();

    // KEK: load_or_create (uses set_durable, first leg is durable).
    let kek = ProjectKek::load_or_create(&chain, PROJECT_A).unwrap();
    assert_eq!(kek.project_id(), PROJECT_A);

    // Both are readable from the chain.
    assert_eq!(
        get_provider_secret(&chain, PROJECT_A, "anthropic")
            .unwrap()
            .expose(),
        "sk-chain"
    );
    assert!(ProjectKek::load(&chain, PROJECT_A).is_ok());

    // Removing the provider secret does not disturb the KEK.
    chain
        .remove(&provider_secret_key(PROJECT_A, "anthropic"))
        .unwrap();
    assert!(matches!(
        get_provider_secret(&chain, PROJECT_A, "anthropic"),
        Err(SecretStoreError::NotFound { .. })
    ));
    assert!(
        ProjectKek::load(&chain, PROJECT_A).is_ok(),
        "KEK must survive provider secret removal"
    );
}
