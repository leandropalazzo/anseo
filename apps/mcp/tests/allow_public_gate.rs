//! GA criterion mcp-9 — `--allow-public` refuses without API key.
//!
//! The guard lives in `apps/mcp/src/main.rs` and runs before any port bind,
//! so the process exits immediately (exit code 1) with a diagnostic on stderr.

#[test]
fn allow_public_without_key_exits_1() {
    let bin = env!("CARGO_BIN_EXE_anseo-mcp");
    let output = std::process::Command::new(bin)
        .env_remove("ANSEO_API_KEY")
        .env("RUST_LOG", "error")
        .args(["--allow-public", "--transport", "http+sse"])
        .output()
        .expect("failed to spawn anseo-mcp");
    assert_eq!(
        output.status.code(),
        Some(1),
        "--allow-public without ANSEO_API_KEY must exit 1"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--allow-public requires ANSEO_API_KEY"),
        "expected error message in stderr; got: {stderr}"
    );
}
