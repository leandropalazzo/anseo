//! FR-49 `get_citations` tool — wires to GET /v1/citations/summary.

use super::{parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;
use opengeo_wire_schema::mcp::tools::{
    CitationSummaryItem, GetCitationsInput, GetCitationsOutput, Window,
};
use ulid::Ulid;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/get_citations.input.json");

pub struct GetCitations;

fn make_upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(opengeo_wire_schema::mcp::McpError {
        kind: opengeo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: Ulid::new().to_string(),
        upstream: None,
    })
}

impl Tool for GetCitations {
    fn name(&self) -> &'static str {
        "get_citations"
    }

    fn description(&self) -> &'static str {
        "Return the top-N cited domains for a project within a time window, with frequency, source type, and sample prompt-run IDs."
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
        let input = serde_json::from_value::<GetCitationsInput>(args)
            .map_err(|e| make_upstream_err(&format!("invalid params: {e}")))?;

        let window = input.window.unwrap_or(Window::ThirtyDays);
        let days_str = match window {
            Window::SevenDays => "7".to_string(),
            Window::ThirtyDays => "30".to_string(),
            Window::All => "90".to_string(),
        };

        let limit = input.top_n.unwrap_or(50);
        let limit_str = limit.to_string();

        let resp = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                api.get("/v1/citations/summary")
                    .query(&[("days", days_str.as_str()), ("limit", limit_str.as_str())])
                    .send()
                    .await
            })
        })
        .map_err(|e| make_upstream_err(&e.to_string()))?;

        if !resp.status().is_success() {
            // Story 36.5: 404 from a project-scoped call → UnknownProject.
            if let Some(e) = super::map_project_not_found(resp.status(), api) {
                return Err(e);
            }
            return Err(make_upstream_err(&format!(
                "upstream /v1/citations/summary returned {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_upstream_err(&format!("failed to parse response: {e}")))?;

        let raw_domains = body
            .get("domains")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut items: Vec<CitationSummaryItem> = Vec::new();

        for d in &raw_domains {
            let domain = d
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let frequency = d.get("frequency").and_then(|v| v.as_u64()).unwrap_or(0);
            let source_type = d
                .get("source_type")
                .and_then(|v| v.as_str())
                .unwrap_or("other")
                .to_string();
            let sample_prompt_run_ids = d
                .get("sample_prompt_run_ids")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|id| id.as_str().map(|s| s.to_string()))
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default();

            items.push(CitationSummaryItem {
                domain,
                frequency,
                source_type,
                sample_prompt_run_ids,
            });
        }

        let output = GetCitationsOutput {
            window,
            items,
            trace_id: Ulid::new().to_string(),
        };

        serde_json::to_value(output).map_err(|e| make_upstream_err(&e.to_string()))
    }
}
