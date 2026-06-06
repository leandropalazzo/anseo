//! Story 41.1 — `anseo plugin search` smoke tests.
//!
//! These drive the built binary through `assert_cmd` and are fully hermetic:
//!   * the local-registry path uses an on-disk fixture tree (no network);
//!   * the live-registry path is pointed at an unreachable reserved address
//!     (`http://127.0.0.1:1`) which fails to connect *instantly* and never
//!     leaves the host, so the AC4 "registry unreachable" branch is exercised
//!     without a real network call.
//!
//! The verification/checksum/malformed-index matrix is covered hermetically in
//! `crates/plugin-host/src/registry/tests.rs`; here we only assert the CLI
//! wiring + UX (zero-state, graceful failure).

use std::path::Path;

use assert_cmd::Command;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// A minimal local registry tree with one searchable plugin row.
fn fixture_registry(root: &Path) {
    write(
        root,
        "index.toml",
        "schema_version = \"1\"\n\n\
         [[plugin]]\n\
         id = \"priya.perplexity-pro\"\n\
         version = \"0.3.1\"\n\
         description = \"Higher-recall extraction\"\n\
         sha256 = \"00\"\n",
    );
}

#[test]
fn local_registry_search_matches_and_zero_states() {
    let tmp = tempfile::tempdir().unwrap();
    fixture_registry(tmp.path());

    // A matching query prints the plugin row.
    Command::cargo_bin("anseo")
        .unwrap()
        .args(["plugin", "search", "perplexity", "--registry"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("priya.perplexity-pro@0.3.1"));

    // A non-matching query is a clean zero-state, not an error.
    Command::cargo_bin("anseo")
        .unwrap()
        .args(["plugin", "search", "nonexistent-xyz", "--registry"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("no plugins match"));
}

/// Story 41.1 AC7 — live smoke test. `#[ignore]` by default because it makes a
/// real network call (the rest of the suite is hermetic). Run explicitly in a
/// dedicated CI job once `github.com/anseo/plugin-registry` exists:
///
///   cargo test -p anseo-cli --test plugin_search_smoke -- --ignored
///
/// Asserts the real registry index is fetchable (HTTP 200) and search exits 0.
#[test]
#[ignore = "live network: requires github.com/anseo/plugin-registry; run in CI smoke job"]
fn live_registry_index_is_reachable() {
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("anseo")
        .unwrap()
        .env("ANSEO_PLUGIN_HOME", home.path())
        .args(["plugin", "search", "anything", "--refresh"])
        .assert()
        .success();
}

#[test]
fn live_registry_unreachable_is_graceful() {
    // Reserved port 1 on loopback: connection is refused immediately. No
    // external network traffic. Use an isolated plugin home so the cache is
    // empty (no stale fallback).
    let home = tempfile::tempdir().unwrap();
    Command::cargo_bin("anseo")
        .unwrap()
        .env("ANSEO_PLUGIN_REGISTRY_URL", "http://127.0.0.1:1")
        .env("ANSEO_PLUGIN_HOME", home.path())
        .args(["plugin", "search", "anything", "--refresh"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("registry unreachable"));
}
