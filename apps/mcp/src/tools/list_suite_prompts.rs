//! Story 40.5 — `list_suite_prompts` MCP tool.
//!
//! Thin passthrough to `GET /v1/suite/prompts`: returns the canonical
//! benchmark-comparability slugs so agent workflows can validate or select a
//! shared cohort slug without scraping docs.

use super::Tool;
use crate::error::McpToolError;
use crate::http_client::ApiClient;
use serde_json::json;
use ulid::Ulid;

fn make_upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(anseo_wire_schema::mcp::McpError {
        kind: anseo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: Ulid::new().to_string(),
        upstream: None,
    })
}

pub struct ListSuitePrompts;

impl Tool for ListSuitePrompts {
    fn name(&self) -> &'static str {
        "list_suite_prompts"
    }

    fn description(&self) -> &'static str {
        "List the canonical GEO benchmark prompt slugs (`slug`, `version`, `description`) so external instrumentation can align runs to shared benchmark cohorts."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        _args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let resp = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { api.get("/v1/suite/prompts").send().await })
        })
        .map_err(|e| make_upstream_err(&e.to_string()))?;

        if !resp.status().is_success() {
            return Err(make_upstream_err(&format!(
                "upstream /v1/suite/prompts returned {}",
                resp.status()
            )));
        }

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_upstream_err(&format!("failed to parse response: {e}")))
    }
}
