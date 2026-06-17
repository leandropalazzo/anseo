use assert_cmd::Command;
use predicates::str::contains;

fn anseo() -> Command {
    Command::cargo_bin("anseo").expect("anseo binary built")
}

#[test]
fn suite_list_prints_known_canonical_slugs() {
    let assert = anseo().args(["suite", "list"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&"geo-v1/best-vector-db"));
    assert!(lines.contains(&"geo-v1/llm-observability-tools"));
}

#[test]
fn suite_check_returns_zero_for_canonical_slug() {
    anseo()
        .args(["suite", "check", "geo-v1/best-vector-db"])
        .assert()
        .success()
        .stdout(contains("geo-v1/best-vector-db"));
}

#[test]
fn suite_check_returns_one_for_non_canonical_slug() {
    anseo()
        .args(["suite", "check", "custom/my-slug"])
        .assert()
        .code(1)
        .stderr(contains("suite check failed"));
}
