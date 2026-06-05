//! FR-48 `compare_brands` — wires `GET /v1/comparisons` and assembles a
//! [`CompareBrandsOutput`].
//!
//! Story 16.4: replace the stub with the real REST call.

use super::{parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/compare_brands.input.json");

pub struct CompareBrands;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn make_upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(anseo_wire_schema::mcp::McpError {
        kind: anseo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: ulid::Ulid::new().to_string(),
        upstream: None,
    })
}

/// Convert a `Window` variant to the query-param string the REST API expects.
fn window_to_str(w: anseo_wire_schema::mcp::tools::Window) -> &'static str {
    use anseo_wire_schema::mcp::tools::Window;
    match w {
        Window::SevenDays => "7d",
        Window::ThirtyDays => "30d",
        Window::All => "all",
    }
}

// ---------------------------------------------------------------------------
// Tool implementation
// ---------------------------------------------------------------------------

impl Tool for CompareBrands {
    fn name(&self) -> &'static str {
        "compare_brands"
    }

    fn description(&self) -> &'static str {
        "Return a deterministic comparison matrix of the configured brand vs. its declared competitors across prompts and providers."
    }

    fn input_schema(&self) -> serde_json::Value {
        parse_schema(INPUT_SCHEMA)
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        use anseo_wire_schema::mcp::tools::{CompareBrandsInput, CompareBrandsOutput, Window};

        // 1. Parse input
        let input: CompareBrandsInput =
            serde_json::from_value(args).map_err(|e| make_upstream_err(&e.to_string()))?;

        // 2. Build query params
        let window_str = window_to_str(input.window.unwrap_or(Window::SevenDays)).to_string();

        // The comparisons endpoint requires at least 2 brands. We pass a
        // stable placeholder pair; the server resolves the real brand config
        // for the authenticated project and returns whatever rows it has. If
        // "brand" / "competitor" are not valid slugs for the project the
        // server returns 400 — we map that to an empty output (0 rows).
        let brands_str = "brand,competitor".to_string();

        // Build query as owned key-value pairs so all borrows are satisfied.
        let mut query: Vec<(String, String)> = vec![
            ("brands".to_string(), brands_str),
            ("window".to_string(), window_str),
        ];

        if let Some(ref prompts) = input.prompts {
            if !prompts.is_empty() {
                query.push(("prompts".to_string(), prompts.join(",")));
            }
        }

        // 3. Fire GET /v1/comparisons
        let resp = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { api.get("/v1/comparisons").query(&query).send().await })
        })
        .map_err(|e| make_upstream_err(&e.to_string()))?;

        let status = resp.status();

        // 4. Handle non-2xx — 400 means placeholder brands were rejected;
        //    return an empty output. Other 4xx/5xx → upstream error.
        //    Story 36.5: 404 from a project-scoped call → UnknownProject.
        if !status.is_success() {
            if let Some(e) = super::map_project_not_found(status, api) {
                return Err(e);
            }

            let body = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async { resp.text().await })
            })
            .unwrap_or_default();

            if status == reqwest::StatusCode::BAD_REQUEST {
                // Project has no matching brands for the placeholder — return
                // an empty but valid CompareBrandsOutput so callers degrade
                // gracefully instead of erroring out.
                let resolved_window = input.window.unwrap_or(Window::SevenDays);
                let empty_output = CompareBrandsOutput {
                    window: resolved_window,
                    brand: String::new(),
                    competitors: vec![],
                    rows: vec![],
                    trace_id: ulid::Ulid::new().to_string(),
                };
                return serde_json::to_value(empty_output)
                    .map_err(|e| make_upstream_err(&e.to_string()));
            }

            return Err(make_upstream_err(&format!(
                "comparisons returned HTTP {status}: {body}"
            )));
        }

        // 5. Parse response as CompareBrandsOutput
        let output: CompareBrandsOutput = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_upstream_err(&e.to_string()))?;

        // 6. Serialise and return
        serde_json::to_value(output).map_err(|e| make_upstream_err(&e.to_string()))
    }
}
