//! Integration tests for `ogeo mcp install-config`.

use opengeo_cli::commands::mcp::{run_install_config, InstallConfigArgs};
use tempfile::tempdir;

#[test]
fn install_config_claude_desktop() {
    let dir = tempdir().expect("tempdir");
    let config_path = dir.path().join("test_config.json");

    run_install_config(InstallConfigArgs {
        client: "claude-desktop".into(),
        config_path: Some(config_path.clone()),
        api_key: Some("testkey123".into()),
    })
    .expect("run_install_config should succeed");

    let raw = std::fs::read_to_string(&config_path).expect("config file written");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    // Top-level key "mcpServers" exists
    assert!(value.get("mcpServers").is_some(), "missing mcpServers key");

    // "mcpServers.opengeo.command" == "opengeo-mcp"
    let command = &value["mcpServers"]["opengeo"]["command"];
    assert_eq!(
        command, "opengeo-mcp",
        "unexpected command value: {command}"
    );

    // "mcpServers.opengeo.env.OPENGEO_API_KEY" == "testkey123"
    let api_key = &value["mcpServers"]["opengeo"]["env"]["OPENGEO_API_KEY"];
    assert_eq!(
        api_key, "testkey123",
        "unexpected OPENGEO_API_KEY value: {api_key}"
    );
}

#[test]
fn install_config_placeholder_when_no_api_key() {
    let dir = tempdir().expect("tempdir");
    let config_path = dir.path().join("no_key_config.json");

    run_install_config(InstallConfigArgs {
        client: "claude-desktop".into(),
        config_path: Some(config_path.clone()),
        api_key: None,
    })
    .expect("run_install_config should succeed without api_key");

    let raw = std::fs::read_to_string(&config_path).expect("config file written");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");

    let api_key = &value["mcpServers"]["opengeo"]["env"]["OPENGEO_API_KEY"];
    assert_eq!(
        api_key, "YOUR_API_KEY_HERE",
        "expected placeholder when no key provided, got: {api_key}"
    );
}

#[test]
fn install_config_unknown_client_returns_err() {
    let dir = tempdir().expect("tempdir");
    let config_path = dir.path().join("unused.json");

    let result = run_install_config(InstallConfigArgs {
        client: "unknown-client".into(),
        config_path: Some(config_path),
        api_key: None,
    });

    assert!(result.is_err(), "expected Err for unknown client");
}
