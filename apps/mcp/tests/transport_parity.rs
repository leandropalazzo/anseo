//! Transport parity test (Story 16.6, AC: transport-parity).
//!
//! Verifies that the HTTP+SSE transport and the stdio `Dispatcher::dispatch()`
//! path return identical `tools/list` responses (tools array sorted by name).
//! The test is offline-safe: `tools/list` never makes upstream API calls.

use std::sync::Arc;

use anseo_mcp::dispatch::Dispatcher;
use anseo_mcp::http_client::ApiClient;
use anseo_mcp::protocol::{Id, Request};
use anseo_mcp::transport::http as http_transport;

/// Build a no-op `ApiClient` pointing at a dummy loopback URL.  The client is
/// never actually used during `tools/list` — it is constructed only to satisfy
/// `Dispatcher::new`.
fn dummy_api_client() -> ApiClient {
    ApiClient::new(
        "http://127.0.0.1:9999".to_string(),
        String::new(),
        "test-project".to_string(),
    )
    .expect("dummy ApiClient construction must not fail")
}

/// Build a `tools/list` JSON-RPC request.
fn tools_list_request() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": 1
    })
}

#[tokio::test]
async fn transport_parity() {
    // -----------------------------------------------------------------------
    // 1. Collect the expected tools list via direct Dispatcher::dispatch().
    // -----------------------------------------------------------------------
    let api = dummy_api_client();
    let dispatcher_direct = Dispatcher::new(api.clone());

    let req_direct = Request {
        _jsonrpc: "2.0".to_string(),
        method: "tools/list".to_string(),
        params: None,
        id: Some(Id::Num(1)),
    };

    let direct_tools: Vec<serde_json::Value> = {
        use anseo_mcp::dispatch::Outbound;
        match dispatcher_direct.dispatch(req_direct) {
            Outbound::Success(resp) => resp
                .result
                .get("tools")
                .and_then(|v| v.as_array())
                .cloned()
                .expect("tools/list must return a 'tools' array"),
            other => panic!(
                "expected Outbound::Success, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    };

    // -----------------------------------------------------------------------
    // 2. Start the HTTP transport on an ephemeral port (port 0).
    // -----------------------------------------------------------------------

    // Bind to port 0 to get an OS-assigned port, then immediately release the
    // listener so the transport can bind to it.  This is a TOCTOU window but
    // is standard test practice and vanishingly unlikely to fail in CI.
    let ephemeral_addr = {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind to port 0");
        listener.local_addr().expect("local_addr")
        // listener is dropped here; port is now free
    };

    let api2 = dummy_api_client();
    let dispatcher_http = Arc::new(Dispatcher::new(api2));

    // Spawn the HTTP server in the background.
    let server_addr = ephemeral_addr;
    tokio::spawn(async move {
        http_transport::run(
            dispatcher_http,
            server_addr,
            false,         // require_api_key
            String::new(), // api_key (unused when require_api_key=false)
        )
        .await
        .expect("HTTP transport must not error during test");
    });

    // Give the server a moment to start.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // -----------------------------------------------------------------------
    // 3. Send the same tools/list request over HTTP POST.
    // -----------------------------------------------------------------------
    let client = reqwest::Client::new();
    let url = format!("http://{}/mcp", server_addr);

    let http_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&tools_list_request())
        .send()
        .await
        .expect("POST /mcp must succeed");

    assert_eq!(http_resp.status(), 200, "POST /mcp must return HTTP 200");

    let body: serde_json::Value = http_resp
        .json()
        .await
        .expect("response body must be valid JSON");

    let http_tools: Vec<serde_json::Value> = body
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|v| v.as_array())
        .cloned()
        .expect("HTTP tools/list response must contain result.tools array");

    // -----------------------------------------------------------------------
    // 4. Compare tools sorted by name — order must not matter.
    // -----------------------------------------------------------------------
    fn sorted_by_name(mut tools: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
        tools.sort_by(|a, b| {
            let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let b_name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
            a_name.cmp(b_name)
        });
        tools
    }

    let direct_sorted = sorted_by_name(direct_tools);
    let http_sorted = sorted_by_name(http_tools);

    assert_eq!(
        direct_sorted.len(),
        http_sorted.len(),
        "tool count must be identical between transports"
    );

    for (i, (d, h)) in direct_sorted.iter().zip(http_sorted.iter()).enumerate() {
        assert_eq!(
            d.get("name"),
            h.get("name"),
            "tool[{i}] name mismatch between transports"
        );
        assert_eq!(
            d.get("description"),
            h.get("description"),
            "tool[{i}] description mismatch between transports"
        );
        assert_eq!(
            d.get("inputSchema"),
            h.get("inputSchema"),
            "tool[{i}] inputSchema mismatch between transports"
        );
    }
}
