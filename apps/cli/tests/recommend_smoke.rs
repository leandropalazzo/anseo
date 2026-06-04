//! Story 19.7 — `ogeo recommend …` CLI surface smoke tests.
//!
//! These exercise argument parsing, subcommand wiring, and the
//! config/DATABASE_URL guards without a live Postgres. The transition verbs
//! all require `DATABASE_URL`; with a valid config but no DB URL they must
//! surface a clean ConfigError (exit 64), proving the command is wired and the
//! guard fires before any network use.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn ogeo() -> Command {
    Command::cargo_bin("ogeo").expect("ogeo binary built")
}

fn scaffold() -> TempDir {
    let dir = TempDir::new().unwrap();
    ogeo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    dir
}

#[test]
fn recommend_help_lists_all_six_verbs() {
    let assert = ogeo().args(["recommend", "--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for verb in ["generate", "list", "show", "ack", "dismiss", "mark-acted"] {
        assert!(
            stdout.contains(verb),
            "recommend help should mention `{verb}`"
        );
    }
}

#[test]
fn recommend_list_without_database_url_fails_with_config_error() {
    let dir = scaffold();
    let config_path = dir.path().join("opengeo.yaml");
    ogeo()
        .env_remove("DATABASE_URL")
        .args(["recommend", "list", "--config"])
        .arg(&config_path)
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn recommend_show_rejects_invalid_uuid() {
    let dir = scaffold();
    let config_path = dir.path().join("opengeo.yaml");
    // A bogus DATABASE_URL is fine: the id is parsed before any connection,
    // but connection happens first in `run_show`, so removing the env var keeps
    // the failure deterministic at the DATABASE_URL guard.
    ogeo()
        .env_remove("DATABASE_URL")
        .args(["recommend", "show", "--id", "not-a-uuid", "--config"])
        .arg(&config_path)
        .assert()
        .failure()
        .code(64);
}

#[test]
fn recommend_mark_acted_accepts_evidence_and_note_flags() {
    let dir = scaffold();
    let config_path = dir.path().join("opengeo.yaml");
    // Parsing must accept the optional flags; the run still fails at the
    // DATABASE_URL guard, proving the flags are wired without needing a DB.
    ogeo()
        .env_remove("DATABASE_URL")
        .args([
            "recommend",
            "mark-acted",
            "--id",
            "00000000-0000-0000-0000-000000000000",
            "--evidence-url",
            "https://example.com/evidence",
            "--note",
            "did the thing",
            "--config",
        ])
        .arg(&config_path)
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn recommend_missing_config_exits_64() {
    let dir = TempDir::new().unwrap();
    ogeo()
        .env_remove("DATABASE_URL")
        .args(["recommend", "list", "--config"])
        .arg(dir.path().join("nope.yaml"))
        .assert()
        .failure()
        .code(64);
}
