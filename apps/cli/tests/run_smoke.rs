//! `ogeo prompt run` smoke tests (FR-2, FR-6).
//!
//! Uses `--use-mock-provider` so we never touch the network. Verifies:
//!  - non-empty matrix succeeds with exit 0
//!  - filters limit the matrix
//!  - empty filter result exits non-zero with a clear message

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn anseo() -> Command {
    Command::cargo_bin("anseo").expect("anseo binary built")
}

fn init_project(dir: &TempDir) {
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    // Replace the example commented-out providers with two enabled ones so
    // `prompt run` has cells to execute.
    let cfg = dir.path().join("anseo.yaml");
    std::fs::write(
        &cfg,
        r#"schema_version: '0.1'
brand:
  name: Acme
  variants: [acme]
prompts:
  - name: example-prompt
    text: What are the best vector databases?
  - name: second-prompt
    text: How does Acme compare to its competitors?
providers:
  - name: openai
    model: mock-model
  - name: anthropic
    model: mock-model
"#,
    )
    .unwrap();
}

#[test]
fn prompt_run_mock_full_matrix_exits_zero() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    let cfg = dir.path().join("anseo.yaml");

    let assert = anseo()
        .args(["prompt", "run", "--use-mock-provider", "--config"])
        .arg(&cfg)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    // 2 prompts × 2 providers = 4 lines.
    assert_eq!(
        stdout.lines().filter(|l| l.contains("\"status\":")).count(),
        4
    );
}

#[test]
fn prompt_run_filters_prompt_name() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    let cfg = dir.path().join("anseo.yaml");
    let assert = anseo()
        .args([
            "prompt",
            "run",
            "--use-mock-provider",
            "--prompt",
            "example-prompt",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 2);
    assert!(!stdout.contains("second-prompt"));
}

#[test]
fn prompt_run_no_matching_cells_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    let cfg = dir.path().join("anseo.yaml");
    anseo()
        .args([
            "prompt",
            "run",
            "--use-mock-provider",
            "--prompt",
            "does-not-exist",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .failure()
        .code(64)
        .stderr(contains("no (prompt, provider) cells"));
}

#[test]
fn prompt_run_rejects_unsupported_provider_filter() {
    let dir = TempDir::new().unwrap();
    init_project(&dir);
    let cfg = dir.path().join("anseo.yaml");
    anseo()
        .args(["prompt", "run", "--provider", "not-a-provider", "--config"])
        .arg(&cfg)
        .assert()
        .failure()
        .code(64)
        .stderr(contains("unsupported --provider"));
}
