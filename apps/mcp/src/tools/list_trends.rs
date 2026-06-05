//! FR-50 `list_trends` tool — wires to GET /v1/anomalies.

use super::{parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;
use opengeo_wire_schema::mcp::tools::{
    ListTrendsInput, ListTrendsOutput, TrendDelta, TrendRecord, Window,
};
use ulid::Ulid;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/list_trends.input.json");

pub struct ListTrends;

fn make_upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(opengeo_wire_schema::mcp::McpError {
        kind: opengeo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: Ulid::new().to_string(),
        upstream: None,
    })
}

impl Tool for ListTrends {
    fn name(&self) -> &'static str {
        "list_trends"
    }

    fn description(&self) -> &'static str {
        "List threshold regressions, statistical anomalies, and response-change trends detected for a project in a time window."
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
        let input = serde_json::from_value::<ListTrendsInput>(args)
            .map_err(|e| make_upstream_err(&format!("invalid params: {e}")))?;

        let window_str = match input.window {
            Window::SevenDays => "7d",
            Window::ThirtyDays => "30d",
            Window::All => "all",
        };

        let resp = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                api.get("/v1/anomalies")
                    .query(&[("window", window_str), ("kind", "all")])
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
                "upstream /v1/anomalies returned {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { resp.json().await })
        })
        .map_err(|e| make_upstream_err(&format!("failed to parse response: {e}")))?;

        let raw_anomalies = body
            .get("anomalies")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut trends: Vec<TrendRecord> = raw_anomalies
            .iter()
            .map(|anomaly| {
                let trend_kind = anomaly
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("statistical_anomaly")
                    .to_string();
                let prompt_id = anomaly
                    .get("prompt_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let prompt_name = anomaly
                    .get("prompt_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let provider = anomaly
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let detected_at = anomaly
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                TrendRecord {
                    trend_kind,
                    prompt_id,
                    prompt_name,
                    provider,
                    delta: TrendDelta {
                        metric: String::new(),
                        from: 0.0,
                        to: 0.0,
                    },
                    evidence_prompt_run_ids: vec![],
                    significance: 1.0,
                    detected_at,
                }
            })
            .collect();

        if let Some(min_sig) = input.min_significance {
            trends.retain(|t| t.significance >= min_sig);
        }

        let output = ListTrendsOutput {
            window: input.window,
            trends,
            trace_id: Ulid::new().to_string(),
        };

        serde_json::to_value(output).map_err(|e| make_upstream_err(&e.to_string()))
    }
}
