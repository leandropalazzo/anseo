//! Story 41.6 — plugin surface boundary contract checks.
//!
//! Scope-limited on purpose:
//! - verifies plugin-provider ids travel through the EXISTING `run_prompt` MCP
//!   tool unchanged (no plugin-minted tool seam);
//! - verifies the MCP registry exposes only the two first-party operator plugin
//!   tools (`list_plugins`, `install_plugin`) and no namespaced plugin tool ids.
//!
//! It does NOT scan external plugin repositories. This is host-binary-only
//! enforcement, matching the story's documented scope limitation.

use std::sync::Arc;

use anyhow::Context;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use tokio::sync::{oneshot, Mutex};

use anseo_mcp::http_client::ApiClient;
use anseo_mcp::tools::{registry, run_prompt::RunPrompt, Tool};

#[derive(Clone)]
struct CaptureState {
    seen_provider: Arc<Mutex<Option<String>>>,
}

async fn prompt_runs(
    State(state): State<CaptureState>,
    Json(body): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let provider = body
        .get("provider")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    *state.seen_provider.lock().await = provider;

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "run_id": "01HXPLUGINPASS00000000000000"
        })),
    )
}

async fn spawn_prompt_run_server() -> anyhow::Result<(String, Arc<Mutex<Option<String>>>)> {
    let seen_provider = Arc::new(Mutex::new(None));
    let state = CaptureState {
        seen_provider: seen_provider.clone(),
    };

    let app = Router::new()
        .route("/v1/prompt-runs", post(prompt_runs))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("bind ephemeral prompt-run server")?;
    let addr = listener.local_addr().context("local_addr")?;

    let (ready_tx, ready_rx) = oneshot::channel();
    tokio::spawn(async move {
        let _ = ready_tx.send(());
        axum::serve(listener, app)
            .await
            .expect("serve test prompt-run API");
    });
    let _ = ready_rx.await;

    Ok((format!("http://{addr}"), seen_provider))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plugin_provider_flows_through_existing_run_prompt_tool() {
    let (base_url, seen_provider) = spawn_prompt_run_server().await.unwrap();
    let api = ApiClient::new(base_url, "test-key".into(), "test-project".into()).unwrap();

    let args = serde_json::json!({
        "project": "test-project",
        "prompt": "geo-v1/best-vector-db",
        "providers": ["plugin:anseo-example-provider"]
    });

    let output = RunPrompt
        .call(args, &api)
        .expect("run_prompt should succeed");
    assert_eq!(output["prompt_id"], "geo-v1/best-vector-db");
    assert_eq!(
        output["results"][0]["provider"],
        "plugin:anseo-example-provider"
    );
    assert_eq!(
        output["results"][0]["prompt_run_id"],
        "01HXPLUGINPASS00000000000000"
    );

    let captured = seen_provider.lock().await.clone();
    assert_eq!(
        captured.as_deref(),
        Some("plugin:anseo-example-provider"),
        "run_prompt must forward the plugin provider through the existing MCP tool"
    );
}

#[test]
fn registry_has_no_plugin_minted_tool_ids() {
    let names: Vec<&str> = registry().iter().map(|tool| tool.name()).collect();

    let plugin_related: Vec<&str> = names
        .iter()
        .copied()
        .filter(|name| name.contains("plugin"))
        .collect();
    assert_eq!(
        plugin_related,
        vec!["list_plugins", "install_plugin"],
        "only first-party operator plugin tools may mention plugins in the MCP registry"
    );

    assert!(
        names.iter().all(|name| !name.starts_with("plugin:")),
        "plugins must not mint namespaced MCP tool ids"
    );
}
