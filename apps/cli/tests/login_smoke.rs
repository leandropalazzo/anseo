//! `ogeo login` smoke tests (FR-7, FR-8, FR-11).
//!
//! These exercise behaviour without requiring an OS keychain — the CLI
//! consults the chained SecretStore which terminates in an in-memory leg.
//! Tests focus on:
//!  - unsupported provider name → exit code 66 (auth) with structured error
//!  - non-TTY stdin path (pipe a key in) — secret never echoed back
//!  - empty key → non-zero exit with a clear message

use assert_cmd::Command;
use predicates::str::contains;

fn ogeo() -> Command {
    Command::cargo_bin("ogeo").expect("ogeo binary built")
}

#[test]
fn login_rejects_unsupported_provider() {
    ogeo()
        .args(["login", "not-a-provider"])
        .write_stdin("anything\n")
        .assert()
        .failure()
        .code(66)
        .stderr(contains("unsupported provider"));
}

#[test]
fn login_rejects_empty_stdin() {
    ogeo()
        .args(["login", "openai"])
        .write_stdin("")
        .assert()
        .failure()
        .code(66)
        .stderr(contains("no key provided"));
}

#[test]
fn login_help_lists_subcommand() {
    let assert = ogeo().arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("login"), "help should mention login");
}

#[test]
fn login_error_does_not_leak_stdin_payload() {
    // If `OPENGEO_KEYRING_PASSPHRASE` is unset and the keyring backend
    // refuses (e.g. CI without secret-service), the chained store's
    // age-file leg returns MissingPassphrase. We assert the error string
    // does NOT include the stdin we piped in. Any leak here would mean we
    // accidentally formatted the secret into an error.
    let leaky_value = "leaky-canary-XYZ123\n";
    let output = ogeo()
        .args(["login", "openai"])
        .write_stdin(leaky_value)
        .env_remove("OPENGEO_KEYRING_PASSPHRASE")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stderr.contains("leaky-canary-XYZ123"),
        "stderr leaked secret: {stderr}"
    );
    assert!(
        !stdout.contains("leaky-canary-XYZ123"),
        "stdout leaked secret: {stdout}"
    );
}
