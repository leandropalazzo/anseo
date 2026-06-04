//! FR-51 `search_benchmarks` — queries the **public benchmark dataset**.
//!
//! Story 16.5 lands the body. Project-less per AD-Phase3-MCP-ProjectScoping §4.
//! The privacy floor (architecture-phase3-mcp-server.md §3.6) is enforced
//! structurally: this handler uses [`crate::benchmark_client::BenchmarkClient`]
//! (no API key, no project header), and it transmits **only** the query +
//! provider + window filters — never any local-deployment data.

use super::{parse_schema, Tool};
use crate::benchmark_client::BenchmarkClient;
use crate::error::McpToolError;
use crate::http_client::ApiClient;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/search_benchmarks.input.json");

pub struct SearchBenchmarks;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_err(kind: opengeo_wire_schema::mcp::McpErrorKind, msg: &str) -> McpToolError {
    McpToolError::Upstream(opengeo_wire_schema::mcp::McpError {
        kind,
        message: msg.to_string(),
        details: None,
        request_id: ulid::Ulid::new().to_string(),
        upstream: None,
    })
}

fn window_to_str(w: opengeo_wire_schema::mcp::tools::Window) -> &'static str {
    use opengeo_wire_schema::mcp::tools::Window;
    match w {
        Window::SevenDays => "7d",
        Window::ThirtyDays => "30d",
        Window::All => "all",
    }
}

/// Build the **allow-listed** benchmark query params from the tool input.
///
/// This is the *only* data that leaves for the public benchmark service:
/// the free-text query, an optional provider filter, and the time window.
/// No project, no brand, no API key. Shared with `tests/benchmark_privacy.rs`
/// so the privacy assertion is made against the exact params the tool sends.
pub fn build_benchmark_query(
    input: &opengeo_wire_schema::mcp::tools::SearchBenchmarksInput,
) -> Vec<(String, String)> {
    let mut q: Vec<(String, String)> = vec![("q".to_string(), input.query.clone())];
    if let Some(ref provider) = input.provider {
        if !provider.is_empty() {
            q.push(("provider".to_string(), provider.clone()));
        }
    }
    let window = input
        .time_window
        .map(window_to_str)
        .unwrap_or("30d")
        .to_string();
    q.push(("window".to_string(), window));
    q
}

// ---------------------------------------------------------------------------
// Tool implementation
// ---------------------------------------------------------------------------

impl Tool for SearchBenchmarks {
    fn name(&self) -> &'static str {
        "search_benchmarks"
    }

    fn description(&self) -> &'static str {
        "Search the OpenGEO public benchmark dataset for category findings. Sends no local-deployment data — query and provider filter only."
    }

    fn input_schema(&self) -> serde_json::Value {
        parse_schema(INPUT_SCHEMA)
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        _api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        use opengeo_wire_schema::mcp::tools::{SearchBenchmarksInput, SearchBenchmarksOutput};
        use opengeo_wire_schema::mcp::McpErrorKind;

        // 1. Parse input. We deliberately ignore `_api` — the benchmark path
        //    must never touch the authenticated client (privacy floor).
        let input: SearchBenchmarksInput = serde_json::from_value(args)
            .map_err(|e| make_err(McpErrorKind::ValidationFailed, &e.to_string()))?;

        // 2. Build the headerless benchmark client + allow-listed query.
        let client = BenchmarkClient::from_env()
            .map_err(|e| make_err(McpErrorKind::InternalError, &e.to_string()))?;
        let query = build_benchmark_query(&input);

        // 3. Fire GET against the PUBLIC benchmark service (not local /v1).
        let send = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { client.aggregates_request(&query).send().await })
        });

        // 4. A connect/DNS/timeout failure is UpstreamUnreachable (not
        //    InternalError) per the FR-51 acceptance table (d).
        let resp = match send {
            Ok(r) => r,
            Err(e) => {
                return Err(make_err(
                    McpErrorKind::UpstreamUnreachable,
                    &format!("benchmark service unreachable: {e}"),
                ))
            }
        };

        let status = resp.status();
        if !status.is_success() {
            return Err(make_err(
                McpErrorKind::InternalError,
                &format!("benchmark service returned HTTP {status}"),
            ));
        }

        // 5. Parse the public response into the canonical output shape.
        let output: SearchBenchmarksOutput = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_err(McpErrorKind::InternalError, &e.to_string()))?;

        serde_json::to_value(output)
            .map_err(|e| make_err(McpErrorKind::InternalError, &e.to_string()))
    }
}
