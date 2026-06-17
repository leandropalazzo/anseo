use assert_cmd::Command;

fn anseo() -> Command {
    Command::cargo_bin("anseo").expect("anseo binary built")
}

#[test]
fn mcp_help_lists_tools_verb() {
    let assert = anseo().args(["mcp", "--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("tools"), "mcp help should mention `tools`");
}

#[test]
fn mcp_tools_help_documents_flags() {
    let assert = anseo().args(["mcp", "tools", "--help"]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    for flag in ["--api-url", "--json"] {
        assert!(stdout.contains(flag), "mcp tools --help documents {flag}");
    }
}
