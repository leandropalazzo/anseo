//! Story 40.1 `ingest_run` tool — single-call parity for `POST /v1/ingest/run`.
//!
//! Records an externally-executed prompt run (one the caller ran against a
//! provider outside Anseo's own orchestrator) so it feeds the same
//! extraction → redaction → contribution path as a native run. No streaming:
//! one POST, one structured result. The tool is a thin passthrough — the
//! server endpoint owns all validation, the KEK hard gate, and the redaction
//! boundary; this just forwards the args and surfaces the response (or a
//! structured error) to the MCP client.

use super::{map_project_not_found, parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/ingest_run.input.json");

pub struct IngestRun;

fn make_error(message: &str) -> anseo_wire_schema::mcp::McpError {
    anseo_wire_schema::mcp::McpError {
        kind: anseo_wire_schema::mcp::McpErrorKind::InternalError,
        message: message.to_string(),
        details: None,
        request_id: ulid::Ulid::new().to_string(),
        upstream: None,
    }
}

impl Tool for IngestRun {
    fn name(&self) -> &'static str {
        "ingest_run"
    }

    fn description(&self) -> &'static str {
        "Record an externally-executed prompt run (executed outside Anseo's orchestrator) so it feeds the same extraction, redaction, and benchmark-contribution path as a native run. Single-call, no streaming."
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
        // The endpoint validates the shape; forward the args verbatim so the
        // server stays the single source of truth for the ingest contract.
        let response = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { api.post("/v1/ingest/run").json(&args).send().await })
        })
        .map_err(|e| McpToolError::Upstream(make_error(&e.to_string())))?;

        let status = response.status();
        if status.is_success() {
            return tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async { response.json().await })
            })
            .map_err(|e| McpToolError::Upstream(make_error(&e.to_string())));
        }

        // Map an unknown-project 404 (project header didn't resolve) to the
        // structured UnknownProject error, like the other tools.
        if let Some(err) = map_project_not_found(status, api) {
            return Err(err);
        }

        let body_text = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { response.text().await })
        })
        .unwrap_or_default();
        Err(McpToolError::Upstream(make_error(&format!(
            "HTTP {}: {body_text}",
            status.as_u16()
        ))))
    }
}
