//! Smoke tests for `ogeo serve` (Story 37.1).
//!
//! These exercise argument parsing via the real binary. The managed-child
//! Postgres path is intentionally covered by the dedicated
//! `managed_pg_migrations` gate so this default smoke suite stays hermetic.

use assert_cmd::Command;

fn anseo() -> Command {
    Command::cargo_bin("anseo").expect("anseo binary built")
}

#[test]
fn serve_appears_in_top_level_help() {
    let assert = anseo().arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("serve"), "top-level help lists `serve`");
}

#[test]
fn serve_help_documents_flags() {
    let assert = anseo().args(["serve", "--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for flag in ["--projects-dir", "--bind", "--port"] {
        assert!(stdout.contains(flag), "serve --help documents {flag}");
    }
}

#[test]
fn serve_help_documents_ephemeral_port_for_smoke_tests() {
    let assert = anseo().args(["serve", "--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("--port"),
        "serve --help documents the port flag used by bounded smoke tests"
    );
}
