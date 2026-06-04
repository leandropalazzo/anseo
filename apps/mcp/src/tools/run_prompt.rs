//! FR-46 `run_prompt` tool — wires `POST /v1/prompt-runs` for each requested
//! provider and assembles a [`RunPromptOutput`].
//!
//! Story 16.2: replace the stub with the real REST call.

use super::{parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/run_prompt.input.json");

pub struct RunPrompt;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn make_error(_kind: &str, message: &str) -> opengeo_wire_schema::mcp::McpError {
    opengeo_wire_schema::mcp::McpError {
        kind: opengeo_wire_schema::mcp::McpErrorKind::InternalError,
        message: message.to_string(),
        details: None,
        request_id: ulid::Ulid::new().to_string(),
        upstream: None,
    }
}

// ---------------------------------------------------------------------------
// Tool implementation
// ---------------------------------------------------------------------------

impl Tool for RunPrompt {
    fn name(&self) -> &'static str {
        "run_prompt"
    }

    fn description(&self) -> &'static str {
        "Execute a configured OpenGEO prompt against one or more LLM providers and return per-provider results (mentions, citations, rankings)."
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
        // 1. Parse input
        let input: opengeo_wire_schema::mcp::tools::RunPromptInput = serde_json::from_value(args)
            .map_err(|e| {
            McpToolError::Upstream(make_error("invalid_params", &e.to_string()))
        })?;

        // 2. Resolve provider list — default to ["mock"]
        let providers = input
            .providers
            .clone()
            .unwrap_or_else(|| vec!["mock".to_string()]);

        // 3. Fire one POST per provider
        let mut results: Vec<opengeo_wire_schema::mcp::tools::RunPromptResult> = Vec::new();

        for provider in &providers {
            let body = serde_json::json!({
                "prompt_name": input.prompt,
                "provider": provider,
                "triggered_by": "mcp",
            });

            let response = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { api.post("/v1/prompt-runs").json(&body).send().await })
            })
            .map_err(|e| McpToolError::Upstream(make_error("http_error", &e.to_string())))?;

            if response.status() == reqwest::StatusCode::ACCEPTED {
                // Parse 202 response body
                let parsed: serde_json::Value = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async { response.json().await })
                })
                .map_err(|e| McpToolError::Upstream(make_error("parse_error", &e.to_string())))?;

                let run_id = parsed
                    .get("run_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                results.push(opengeo_wire_schema::mcp::tools::RunPromptResult {
                    prompt_run_id: run_id,
                    provider: provider.clone(),
                    model: provider.clone(),
                    status: opengeo_wire_schema::mcp::tools::RunPromptStatus::Ok,
                    ranking: None,
                    mentions: vec![],
                    citations: vec![],
                    duration_ms: 0,
                    error: None,
                });
            } else {
                // Non-202 → Failed result
                let status_code = response.status().as_u16();
                let body_text = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async { response.text().await })
                })
                .unwrap_or_default();

                results.push(opengeo_wire_schema::mcp::tools::RunPromptResult {
                    prompt_run_id: String::new(),
                    provider: provider.clone(),
                    model: provider.clone(),
                    status: opengeo_wire_schema::mcp::tools::RunPromptStatus::Failed,
                    ranking: None,
                    mentions: vec![],
                    citations: vec![],
                    duration_ms: 0,
                    error: Some(opengeo_wire_schema::mcp::tools::ResultError {
                        kind: "upstream_error".to_string(),
                        message: format!("HTTP {status_code}: {body_text}"),
                    }),
                });
            }
        }

        // 4. Assemble output
        let output = opengeo_wire_schema::mcp::tools::RunPromptOutput {
            prompt_id: input.prompt.clone(),
            results,
            non_deterministic_pipeline: true,
            trace_id: ulid::Ulid::new().to_string(),
        };

        serde_json::to_value(output)
            .map_err(|e| McpToolError::Upstream(make_error("serialization_error", &e.to_string())))
    }
}
