//! P0-121 (test-design Epic 12) — `OPENGEO_API_BIND` non-loopback refusal.
//!
//! Exercises `opengeo_api::check_bind_acceptable` directly. The boot path
//! in `apps/api/src/main.rs` is a thin wrapper around this function plus
//! env-var reads and a DB count, so unit coverage here pins the entire
//! policy without needing a live Postgres.

use opengeo_api::{bootstrap_key_material, check_bind_acceptable};

#[test]
fn loopback_v4_with_zero_keys_no_test_mode_accepts() {
    let result = check_bind_acceptable("127.0.0.1:8080", false, 0);
    assert!(result.is_ok());
}

#[test]
fn loopback_v6_with_zero_keys_no_test_mode_accepts() {
    let result = check_bind_acceptable("[::1]:8080", false, 0);
    assert!(result.is_ok());
}

#[test]
fn non_loopback_with_zero_keys_refuses() {
    let err = check_bind_acceptable("0.0.0.0:8080", false, 0).unwrap_err();
    assert!(err.contains("no active API keys"));
    assert!(err.contains("ogeo api key create"));
}

#[test]
fn non_loopback_with_active_key_no_test_mode_accepts() {
    let result = check_bind_acceptable("0.0.0.0:8080", false, 1);
    assert!(result.is_ok());
}

#[test]
fn non_loopback_with_active_key_but_test_mode_refuses() {
    // Decision 2: OPENGEO_TEST_MODE=1 makes /test/seed reachable
    // unauthenticated. A public bind with TEST_MODE on must refuse
    // regardless of key count.
    let err = check_bind_acceptable("0.0.0.0:8080", true, 5).unwrap_err();
    assert!(err.contains("OPENGEO_TEST_MODE=1"));
    assert!(err.contains("/test/seed"));
}

#[test]
fn loopback_with_test_mode_still_accepts() {
    // Localhost dev workflow with test mode is the intended path; refusal
    // applies only to non-loopback.
    let result = check_bind_acceptable("127.0.0.1:8080", true, 0);
    assert!(result.is_ok());
}

#[test]
fn malformed_bind_addr_returns_descriptive_error() {
    let err = check_bind_acceptable("localhost:8080", false, 1).unwrap_err();
    assert!(err.contains("invalid OPENGEO_API_BIND"));
}

#[test]
fn ipv6_link_local_is_non_loopback() {
    // `fe80::1` is link-local, not loopback. Treated as non-loopback so
    // the key-count gate applies.
    let err = check_bind_acceptable("[fe80::1]:8080", false, 0).unwrap_err();
    assert!(err.contains("no active API keys"));
}

#[test]
fn bootstrap_key_material_derives_hash_and_prefix_for_valid_key() {
    use opengeo_core::api_key::{sha256_hex, DISPLAY_PREFIX_LEN, KEY_PREFIX};
    let plaintext = format!("{KEY_PREFIX}{}", "A".repeat(32));
    let (hash, prefix) = bootstrap_key_material(&plaintext).unwrap();
    // Hash must match the same sha256_hex the auth middleware computes from
    // the wire token, or the seeded key could never authenticate.
    assert_eq!(hash, sha256_hex(&plaintext));
    assert_eq!(hash.len(), 64);
    // Prefix is the first DISPLAY_PREFIX_LEN chars of the random portion.
    assert_eq!(prefix.len(), DISPLAY_PREFIX_LEN);
    assert_eq!(prefix, "A".repeat(DISPLAY_PREFIX_LEN));
}

#[test]
fn bootstrap_key_material_rejects_malformed_shape() {
    // Wrong prefix, too short, and empty must all be refused so a typo'd
    // env value fails loudly instead of seeding an unusable key.
    for bad in ["sk-not-ours", "ogeo_short", "", "ogeo_"] {
        let err = bootstrap_key_material(bad).unwrap_err();
        assert!(err.contains("ogeo_<32 base62>"), "unexpected msg: {err}");
    }
}
