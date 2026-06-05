//! End-to-end CLI smoke tests for FR-10 (`ogeo init`) and FR-12
//! (`ogeo prompt add` / `ogeo prompt list`).
//!
//! Tests spawn the real `ogeo` binary via `assert_cmd` against a temp dir so
//! they exercise argument parsing, exit codes, and stdout/stderr.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn anseo() -> Command {
    Command::cargo_bin("anseo").expect("anseo binary built")
}

#[test]
fn init_in_empty_dir_scaffolds_three_files() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();

    for name in ["anseo.yaml", ".gitignore", "README.md"] {
        let p = dir.path().join(name);
        assert!(p.exists(), "expected {name} to be scaffolded");
        let contents = std::fs::read_to_string(&p).unwrap();
        assert!(!contents.is_empty(), "{name} should not be empty");
    }
}

#[test]
fn init_writes_a_valid_schema_v0_1_config() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();

    // The scaffolded YAML must parse against the v0.1 schema with zero edits.
    let yaml = std::fs::read_to_string(dir.path().join("anseo.yaml")).unwrap();
    let cfg = anseo_core::Config::from_yaml_str(&yaml).expect("scaffold parses");
    assert_eq!(cfg.schema_version, "0.1");
    assert!(
        !cfg.prompts.is_empty(),
        "scaffold has at least one example prompt"
    );
    assert_eq!(cfg.prompts[0].name, "example-prompt");
}

#[test]
fn init_no_overwrite_exits_nonzero_on_preexisting_file() {
    let dir = TempDir::new().unwrap();
    // Pre-create one file.
    std::fs::write(dir.path().join("README.md"), "preexisting\n").unwrap();

    anseo()
        .args(["init", "--no-overwrite", "--dir"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("refusing to overwrite"));

    // No partial scaffold: pre-existing README untouched, others not created.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
        "preexisting\n"
    );
    assert!(!dir.path().join("anseo.yaml").exists());
}

#[test]
fn init_force_overwrites_without_prompt() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("anseo.yaml"), "stale\n").unwrap();

    anseo()
        .args(["init", "--force", "--dir"])
        .arg(dir.path())
        .assert()
        .success();

    let contents = std::fs::read_to_string(dir.path().join("anseo.yaml")).unwrap();
    assert!(contents.starts_with("# Anseo project"));
}

#[test]
fn init_non_interactive_without_force_fails_on_preexisting() {
    // Same as the no-overwrite case but using the default interactive path.
    // assert_cmd spawns a non-TTY subprocess, so init must refuse instead of
    // hanging on a confirm prompt.
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "preexisting\n").unwrap();

    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .failure();
}

#[test]
fn prompt_add_non_interactive_appends_to_yaml() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();

    let config_path = dir.path().join("anseo.yaml");
    anseo()
        .args(["prompt", "add", "--name", "second-prompt", "--text"])
        .arg("Why is the sky blue?")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .success();

    let yaml = std::fs::read_to_string(&config_path).unwrap();
    let cfg = anseo_core::Config::from_yaml_str(&yaml).unwrap();
    assert!(cfg.prompts.iter().any(|p| p.name == "second-prompt"));
}

#[test]
fn prompt_add_rejects_duplicate_name() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    let config_path = dir.path().join("anseo.yaml");

    anseo()
        .args(["prompt", "add", "--name", "example-prompt", "--text"])
        .arg("anything")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .failure()
        .stderr(contains("duplicate prompt name"));
}

#[test]
fn prompt_add_rejects_invalid_slug() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    let config_path = dir.path().join("anseo.yaml");

    anseo()
        .args(["prompt", "add", "--name", "Bad Name", "--text"])
        .arg("anything")
        .arg("--config")
        .arg(&config_path)
        .assert()
        .failure()
        .stderr(contains("slug"));
}

#[test]
fn prompt_add_non_interactive_without_required_flags_fails() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    let config_path = dir.path().join("anseo.yaml");

    anseo()
        .args(["prompt", "add", "--name", "needs-text"])
        .arg("--config")
        .arg(&config_path)
        .assert()
        .failure()
        .stderr(contains("--name and --text"));
}

#[test]
fn prompt_list_table_default_includes_example_prompt() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    let config_path = dir.path().join("anseo.yaml");

    anseo()
        .args(["prompt", "list", "--config"])
        .arg(&config_path)
        .assert()
        .success()
        .stdout(contains("NAME"))
        .stdout(contains("example-prompt"));
}

#[test]
fn prompt_list_json_emits_stable_array() {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    let config_path = dir.path().join("anseo.yaml");

    let assert = anseo()
        .args(["prompt", "list", "--format", "json", "--config"])
        .arg(&config_path)
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "example-prompt");
}

#[test]
fn cli_help_prints_subcommands() {
    let assert = anseo().arg("--help").assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("init"), "help mentions init");
    assert!(stdout.contains("prompt"), "help mentions prompt");
}

#[test]
fn config_error_exits_with_code_64() {
    // Pointing the list command at a non-existent config triggers ConfigError,
    // which must surface as exit code 64 (PRD §11.4).
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["prompt", "list", "--config"])
        .arg(dir.path().join("nope.yaml"))
        .assert()
        .failure()
        .code(64);
}
