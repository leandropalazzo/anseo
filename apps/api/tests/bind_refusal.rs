//! P0-121 (test-design Epic 12) — `OPENGEO_API_BIND` non-loopback refusal.
//!
//! Exercises `opengeo_api::check_bind_acceptable` directly. The boot path
//! in `apps/api/src/main.rs` is a thin wrapper around this function plus
//! env-var reads and a DB count, so unit coverage here pins the entire
//! policy without needing a live Postgres.

use opengeo_api::check_bind_acceptable;

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
