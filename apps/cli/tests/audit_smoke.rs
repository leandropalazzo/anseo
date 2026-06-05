//! Epic 32 — `ogeo audit` CLI smoke tests.

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn anseo() -> Command {
    Command::cargo_bin("anseo").expect("anseo binary built")
}

fn fixture(html: &str) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("page.html");
    std::fs::write(&path, html).unwrap();
    (dir, path)
}

#[test]
fn audit_help_is_wired() {
    anseo()
        .args(["audit", "--help"])
        .assert()
        .success()
        .stdout(contains("--fail-on"))
        .stdout(contains("--format"));
}

#[test]
fn audit_json_emits_per_page_per_rule_findings() {
    let (_dir, path) = fixture(
        r#"
        <html>
          <head>
            <title>How answers work | Acme</title>
            <link rel="canonical" href="https://example.com/a">
            <script type="application/ld+json">{"@context":"https://schema.org"}</script>
          </head>
          <body>
            <h1>How do citations work?</h1>
            <p class="answer">The answer cites sources.</p>
            <h2>References</h2>
            <p>According to the report, citations help.</p>
            <a href="https://research.example/report">Source report</a>
          </body>
        </html>
        "#,
    );

    let assert = anseo()
        .arg("audit")
        .arg(&path)
        .args(["--format", "json"])
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["pages"].as_array().unwrap().len(), 1);
    assert!(json["pages"][0]["findings"].as_array().unwrap().len() >= 9);
    assert_eq!(json["pages"][0]["findings"][0]["category"], "identity");
}

#[test]
fn audit_fail_on_rule_exits_one_with_machine_summary() {
    let (_dir, path) = fixture("<html><body><p>thin page</p></body></html>");
    anseo()
        .arg("audit")
        .arg(&path)
        .args(["--fail-on", "identity.canonical_url"])
        .assert()
        .failure()
        .code(1)
        .stderr(contains("\"rule_id\":\"identity.canonical_url\""));
}
