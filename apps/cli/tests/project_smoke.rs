//! Story 36.6 — `ogeo project …` CLI surface + `--project` precedence smoke
//! tests.
//!
//! These exercise argument parsing, subcommand wiring, the global `--project`
//! flag, and the `DATABASE_URL` guard without a live Postgres. Every project
//! verb is direct-DB and requires `DATABASE_URL`; with it removed they must
//! surface a clean ConfigError (exit 64), proving the command is wired and the
//! guard fires before any network use.
//!
//! The precedence *order* itself is unit-tested in
//! `apps/cli/src/commands/project.rs` (marker round-trip, name/id matching);
//! these binary-level tests prove the verbs and the global flag are reachable.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn anseo() -> Command {
    Command::cargo_bin("anseo").expect("anseo binary built")
}

fn scaffold() -> TempDir {
    let dir = TempDir::new().unwrap();
    anseo()
        .args(["init", "--dir"])
        .arg(dir.path())
        .assert()
        .success();
    dir
}

#[test]
fn project_help_lists_all_three_verbs() {
    let assert = anseo().args(["project", "--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for verb in ["list", "create", "use"] {
        assert!(
            stdout.contains(verb),
            "project help should mention `{verb}`"
        );
    }
}

#[test]
fn project_flag_is_global_and_documented() {
    // The `--project` flag must be visible on the top-level help (global flag).
    let assert = anseo().args(["--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("--project"),
        "top-level help should list the global --project flag"
    );
}

#[test]
fn project_list_without_database_url_fails_with_config_error() {
    anseo()
        .env_remove("DATABASE_URL")
        .args(["project", "list"])
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn project_list_json_flag_is_accepted() {
    // Parsing must accept `--json`; run still fails at the DATABASE_URL guard.
    anseo()
        .env_remove("DATABASE_URL")
        .args(["project", "list", "--json"])
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn project_create_without_database_url_fails_with_config_error() {
    anseo()
        .env_remove("DATABASE_URL")
        .args(["project", "create", "Sunski"])
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn project_create_accepts_variant_and_site_url_flags() {
    // Parsing must accept the optional flags; the run still fails at the
    // DATABASE_URL guard, proving the flags are wired without needing a DB.
    anseo()
        .env_remove("DATABASE_URL")
        .args([
            "project",
            "create",
            "Sunski",
            "--variant",
            "sunski-eyewear",
            "--site-url",
            "https://sunski.com",
        ])
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn project_use_without_database_url_fails_with_config_error() {
    anseo()
        .env_remove("DATABASE_URL")
        .args(["project", "use", "Sunski"])
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn global_project_flag_overrides_on_a_resolving_verb() {
    // `--project` is accepted on a project-resolving verb (crawlers). Without a
    // DB the run still fails at the DATABASE_URL guard, proving the override is
    // parsed and threaded before any resolution work.
    let dir = scaffold();
    let config_path = dir.path().join("anseo.yaml");
    anseo()
        .env_remove("DATABASE_URL")
        .args(["crawlers", "--project", "Acme", "--config"])
        .arg(&config_path)
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}

#[test]
fn global_project_flag_position_independent() {
    // A global flag must parse whether it precedes or follows the subcommand.
    anseo()
        .env_remove("DATABASE_URL")
        .args(["--project", "Acme", "project", "list"])
        .assert()
        .failure()
        .code(64)
        .stderr(contains("DATABASE_URL"));
}
