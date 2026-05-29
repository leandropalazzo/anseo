//! Smoke tests for Phase 2 `ogeo schedule` CLI declarations.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn ogeo() -> Command {
    Command::cargo_bin("ogeo").expect("ogeo binary built")
}

fn write_config(dir: &TempDir) -> std::path::PathBuf {
    let cfg = dir.path().join("opengeo.yaml");
    std::fs::write(
        &cfg,
        r#"schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: example-prompt
    text: What are the best vector databases?
  - name: second-prompt
    text: How does Acme compare to competitors?
providers:
  - name: openai
  - name: anthropic
"#,
    )
    .unwrap();
    cfg
}

#[test]
fn schedule_add_promotes_config_to_v0_2_and_lists() {
    let dir = TempDir::new().unwrap();
    let cfg = write_config(&dir);

    ogeo()
        .args([
            "schedule",
            "add",
            "--name",
            "daily-watch",
            "--cron",
            "daily",
            "--prompt",
            "example-prompt",
            "--provider",
            "openai",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .success()
        .stderr(contains("projected monthly cost"));

    let yaml = std::fs::read_to_string(&cfg).unwrap();
    let parsed = opengeo_core::Config::from_yaml_str(&yaml).unwrap();
    assert_eq!(parsed.schema_version, "0.2");
    assert_eq!(parsed.schedules[0].name, "daily-watch");

    ogeo()
        .args(["schedule", "list", "--config"])
        .arg(&cfg)
        .assert()
        .success()
        .stdout(contains("daily-watch"));
}

#[test]
fn schedule_add_rejects_density_cap_violation() {
    let dir = TempDir::new().unwrap();
    let cfg = write_config(&dir);

    ogeo()
        .args([
            "schedule",
            "add",
            "--name",
            "too-fast",
            "--cron",
            "every 5 minutes",
            "--prompt",
            "example-prompt",
            "--provider",
            "openai",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .failure()
        .code(64)
        .stderr(contains("density cap"));
}

#[test]
fn schedule_add_requires_ack_for_expensive_projection() {
    let dir = TempDir::new().unwrap();
    let cfg = write_config(&dir);

    ogeo()
        .args([
            "schedule",
            "add",
            "--name",
            "expensive-watch",
            "--cron",
            "every 15 minutes",
            "--prompt",
            "example-prompt",
            "--prompt",
            "second-prompt",
            "--provider",
            "openai",
            "--provider",
            "anthropic",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .failure()
        .code(64)
        .stderr(contains("--allow-expensive"));

    ogeo()
        .args([
            "schedule",
            "add",
            "--name",
            "expensive-watch",
            "--cron",
            "every 15 minutes",
            "--prompt",
            "example-prompt",
            "--prompt",
            "second-prompt",
            "--provider",
            "openai",
            "--provider",
            "anthropic",
            "--allow-expensive",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .success();

    let parsed =
        opengeo_core::Config::from_yaml_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
    assert!(parsed.schedules[0].projection_acknowledged_at.is_some());
}

#[test]
fn schedule_remove_updates_yaml() {
    let dir = TempDir::new().unwrap();
    let cfg = write_config(&dir);
    ogeo()
        .args([
            "schedule",
            "add",
            "--name",
            "daily-watch",
            "--cron",
            "daily",
            "--prompt",
            "example-prompt",
            "--provider",
            "openai",
            "--config",
        ])
        .arg(&cfg)
        .assert()
        .success();

    ogeo()
        .args(["schedule", "remove", "--name", "daily-watch", "--config"])
        .arg(&cfg)
        .assert()
        .success();

    let parsed =
        opengeo_core::Config::from_yaml_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
    assert!(parsed.schedules.is_empty());
}

#[test]
fn worker_status_is_explicit_placeholder() {
    ogeo()
        .args(["worker", "status"])
        .assert()
        .success()
        .stdout(contains("not-running"))
        .stdout(contains("Story 10.2"));
}
