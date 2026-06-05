//! FR-51 benchmark **privacy floor** — GA criterion `[mcp-6]`.
//!
//! Snapshots the outbound `search_benchmarks` HTTP request and asserts it
//! carries NO local-deployment identifiers (no API key, no project/brand
//! header), a brand-free User-Agent, and only the allow-listed query params.
//! Also asserts the unreachable-service failure maps to `UpstreamUnreachable`
//! (FR-51 acceptance (d)).

use anseo_mcp::benchmark_client::{benchmark_user_agent, BenchmarkClient};
use anseo_mcp::tools::search_benchmarks::build_benchmark_query;
use anseo_wire_schema::mcp::tools::{SearchBenchmarksInput, Window};

fn sample_input() -> SearchBenchmarksInput {
    SearchBenchmarksInput {
        // A query that *looks* like it could carry a brand, to prove only the
        // user-supplied text (never a resolved project/brand identity) is sent.
        query: "best project management tools".to_string(),
        provider: Some("openai".to_string()),
        time_window: Some(Window::ThirtyDays),
    }
}

/// (a)+(b) The outbound request leaks no project/brand identifiers and no key.
#[test]
fn privacy_floor_request_carries_no_local_identifiers() {
    let client = BenchmarkClient::with_base_url("https://benchmark.opengeo.dev".into()).unwrap();
    let query = build_benchmark_query(&sample_input());
    let req = client.aggregates_request(&query).build().unwrap();

    // No auth, no project/brand headers — ever.
    let headers = req.headers();
    assert!(
        headers.get(reqwest::header::AUTHORIZATION).is_none(),
        "benchmark request must NOT carry an Authorization header"
    );
    for forbidden in ["x-opengeo-project", "project-id", "x-project"] {
        assert!(
            headers.get(forbidden).is_none(),
            "benchmark request must NOT carry the `{forbidden}` header"
        );
    }

    // Only the allow-listed query keys may appear.
    let keys: Vec<String> = req
        .url()
        .query_pairs()
        .map(|(k, _)| k.into_owned())
        .collect();
    for k in &keys {
        assert!(
            matches!(k.as_str(), "q" | "provider" | "window"),
            "unexpected query param `{k}` — only q/provider/window are allow-listed"
        );
    }
    // Sanity: the user-supplied filters are present, nothing else.
    assert!(keys.contains(&"q".to_string()));
    assert!(keys.contains(&"window".to_string()));

    // Hits the PUBLIC benchmark host, not a local /v1 surface.
    assert_eq!(req.url().host_str(), Some("benchmark.opengeo.dev"));
    assert!(req.url().path().starts_with("/v1/benchmark/"));
}

/// (a) The User-Agent is fixed and contains no brand name.
#[test]
fn user_agent_is_brand_free() {
    let ua = benchmark_user_agent();
    assert!(ua.starts_with("anseo-mcp/"));
    assert!(ua.ends_with(" benchmark-search"));

    let client = BenchmarkClient::with_base_url("https://benchmark.opengeo.dev".into()).unwrap();
    let req = client
        .aggregates_request(&build_benchmark_query(&sample_input()))
        .build()
        .unwrap();
    let sent_ua = req
        .headers()
        .get(reqwest::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        sent_ua, ua,
        "benchmark request must use the fixed brand-free UA"
    );
}

/// (d) Unreachable benchmark service → `UpstreamUnreachable` (not internal).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unreachable_benchmark_service_maps_to_upstream_unreachable() {
    use anseo_mcp::error::McpToolError;
    use anseo_mcp::http_client::ApiClient;
    use anseo_mcp::tools::{search_benchmarks::SearchBenchmarks, Tool};
    use anseo_wire_schema::mcp::McpErrorKind;

    // Point the benchmark client at a closed port → connection refused.
    std::env::set_var("ANSEO_BENCHMARK_URL", "http://127.0.0.1:9");

    // The authed ApiClient is required by the signature but must be ignored.
    let api = ApiClient::new(
        "http://127.0.0.1:8788".into(),
        "unused-key".into(),
        "default".into(),
    )
    .unwrap();

    let args = serde_json::json!({ "query": "anything" });
    let err = SearchBenchmarks
        .call(args, &api)
        .expect_err("unreachable benchmark service must error");

    match err {
        McpToolError::Upstream(e) => assert!(
            matches!(e.kind, McpErrorKind::UpstreamUnreachable),
            "expected UpstreamUnreachable, got {:?}",
            e.kind
        ),
        other => panic!("expected Upstream(UpstreamUnreachable), got {other:?}"),
    }

    std::env::remove_var("ANSEO_BENCHMARK_URL");
}
