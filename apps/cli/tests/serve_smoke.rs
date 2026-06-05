//! Smoke tests for `ogeo serve` (Story 37.1).
//!
//! These exercise argument parsing and the pre-flight `DATABASE_URL` guard via
//! the real binary — hermetic: no Postgres, no port bind. The supervisor's
//! pure helpers (bind/config-path resolution, shutdown-signal wiring) are
//! unit-tested in `apps/cli/src/commands/serve.rs`.

use assert_cmd::Command;
use predicates::str::contains;

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
fn serve_without_database_url_fails_with_config_code() {
    // No DATABASE_URL → pre-flight guard refuses before any bind/boot work.
    // ConfigError surfaces as exit code 64 (PRD §11.4). Clearing the env var
    // keeps the test hermetic regardless of the developer's shell.
    anseo()
        .env_remove("DATABASE_URL")
        .arg("serve")
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}
