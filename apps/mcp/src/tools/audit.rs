//! Roadmap Epic 32 Story 4 — `audit` tool. Wires to POST /v1/audit, the same
//! in-tree citation-readiness engine the CLI (`ogeo audit`) and the `/audit`
//! dashboard use (CLI ⇄ Web ⇄ MCP parity).

use super::{parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;
use opengeo_wire_schema::mcp::tools::{AuditFindingRecord, AuditInput, AuditOutput};
use ulid::Ulid;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/audit.input.json");

pub struct Audit;

fn make_upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(opengeo_wire_schema::mcp::McpError {
        kind: opengeo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: Ulid::new().to_string(),
        upstream: None,
    })
}

impl Tool for Audit {
    fn name(&self) -> &'static str {
        "audit"
    }

    fn description(&self) -> &'static str {
        "Crawl a URL/sitemap and score citation-readiness against open, in-tree heuristics (Identity, Extractability, Corroboration). Optionally apply CI-gate thresholds."
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
        let input = serde_json::from_value::<AuditInput>(args)
            .map_err(|e| make_upstream_err(&format!("invalid params: {e}")))?;

        let mut body = serde_json::json!({ "target": input.target });
        if let Some(max_pages) = input.max_pages {
            body["max_pages"] = serde_json::json!(max_pages);
        }
        if !input.fail_on.is_empty() {
            body["fail_on"] = serde_json::json!(input.fail_on);
        }

        let resp = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { api.post("/v1/audit").json(&body).send().await })
        })
        .map_err(|e| make_upstream_err(&e.to_string()))?;

        if !resp.status().is_success() {
            // Story 36.5: 404 from a project-scoped call → UnknownProject.
            if let Some(e) = super::map_project_not_found(resp.status(), api) {
                return Err(e);
            }
            return Err(make_upstream_err(&format!(
                "upstream /v1/audit returned {}",
                resp.status()
            )));
        }

        let report: serde_json::Value = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_upstream_err(&format!("failed to parse response: {e}")))?;

        let mut findings: Vec<AuditFindingRecord> = Vec::new();
        for page in report
            .get("pages")
            .and_then(|p| p.as_array())
            .into_iter()
            .flatten()
        {
            let page_url = page
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            for f in page
                .get("findings")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                let status = f.get("status").and_then(|v| v.as_str()).unwrap_or("");
                if status == "pass" {
                    continue;
                }
                findings.push(AuditFindingRecord {
                    page_url: page_url.clone(),
                    rule_id: f
                        .get("rule_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    category: f
                        .get("category")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    severity: f
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    status: status.to_string(),
                    message: f
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }

        let output = AuditOutput {
            target: report
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            overall_score: report
                .get("overall_score")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u8,
            pages_crawled: report
                .get("pages")
                .and_then(|p| p.as_array())
                .map(|a| a.len() as u32)
                .unwrap_or(0),
            findings,
            gate_passed: report
                .get("gate")
                .and_then(|g| g.get("passed"))
                .and_then(|v| v.as_bool()),
            trace_id: Ulid::new().to_string(),
        };

        serde_json::to_value(output)
            .map_err(|e| make_upstream_err(&format!("failed to serialize output: {e}")))
    }
}
