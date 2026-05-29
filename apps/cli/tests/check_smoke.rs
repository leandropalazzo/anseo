//! `ogeo check visibility` smoke tests (FR-15, P0-009).
//!
//! Phase 1 today: the subcommand exists with the correct argument shape, but
//! ranking logic ships in Story 3.2 — the stub currently returns
//! `OpenGeoError::Provider { kind: NetworkError, ... }` which maps to
//! `ExitCode::ProviderError = 2`. These tests pin the contract that's
//! observable today; Story 3.2 will extend them with the full
//! (provider-result × ranking-outcome) matrix.
//!
//! Exit code reference (`crates/core/src/error.rs`):
//!  - `Success`              = 0
//!  - `VisibilityCheckFailed`= 1  (Story 3.2: brand rank > threshold)
//!  - `ProviderError`        = 2  (Phase 1 stub: every invocation hits this)
//!  - `ConfigError`          = 64 (clap usage / invalid config)
//!
//! trace: P0-009 (FR-15)

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn ogeo() -> Command {
    Command::cargo_bin("ogeo").expect("ogeo binary built")
}

fn init_project(dir: &TempDir) {
    ogeo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
fn check_visibility_help_lists_subcommand() {
    ogeo()
        .args(["check", "--help"])
        .assert()
        .success()
        .stdout(contains("visibility"));
}

#[test]
fn check_visibility_help_lists_required_flags() {
    let assert = ogeo()
        .args(["check", "visibility", "--help"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for flag in [
        "--prompt",
        "--brand",
        "--expect-rank-lte",
        "--no-run",
        "--config",
    ] {
        assert!(
            stdout.contains(flag),
            "expected --help to mention {flag}, got:\n{stdout}"
        );
    }
}

#[test]
fn check_visibility_missing_required_args_fails() {
    // Only --prompt provided; --brand and --expect-rank-lte are missing.
    ogeo()
        .args(["check", "visibility", "--prompt", "demo"])
        .assert()
        .failure();
}

#[test]
fn check_visibility_stub_exits_code_2_with_clear_message() {
    // Phase 1 stub: even with valid args + a real config, the command exits
    // with `ProviderError = 2` and explains the situation. Story 3.2 extends
    // this to the full exit-code matrix (0 / 1 / 2 / 64).
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    let cfg = dir.path().join("opengeo.yaml");

    ogeo()
        .args([
            "check",
            "visibility",
            "--prompt",
            "demo",
            "--brand",
            "Acme",
            "--expect-rank-lte",
            "3",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .failure()
        .code(2)
        .stderr(contains("Story 3.2"));
}

#[test]
fn check_visibility_no_run_flag_still_exits_in_phase_1_stub() {
    // `--no-run` skips a fresh Prompt Run and checks only persisted data.
    // In the Phase 1 stub it has no effect on the outcome — still exits 2.
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    let cfg = dir.path().join("opengeo.yaml");

    ogeo()
        .args([
            "check",
            "visibility",
            "--prompt",
            "demo",
            "--brand",
            "Acme",
            "--expect-rank-lte",
            "3",
            "--no-run",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .failure()
        .code(2);
}
