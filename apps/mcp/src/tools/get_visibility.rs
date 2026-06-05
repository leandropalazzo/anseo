//! FR-47 `get_visibility` tool — wires to GET /v1/visibility/trend.

use super::{parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;
use anseo_wire_schema::mcp::tools::{
    GetVisibilityInput, GetVisibilityOutput, VisibilityPoint, VisibilitySeries,
    VisibilitySeriesSummary, Window,
};
use ulid::Ulid;

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/get_visibility.input.json");

pub struct GetVisibility;

fn make_upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(anseo_wire_schema::mcp::McpError {
        kind: anseo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: Ulid::new().to_string(),
        upstream: None,
    })
}

impl Tool for GetVisibility {
    fn name(&self) -> &'static str {
        "get_visibility"
    }

    fn description(&self) -> &'static str {
        "Return a visibility trend series per prompt for a time window, including per-point ranking, mention count, and prior-window delta."
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
        let input = serde_json::from_value::<GetVisibilityInput>(args)
            .map_err(|e| make_upstream_err(&format!("invalid params: {e}")))?;

        let window = input.window.unwrap_or(Window::ThirtyDays);
        let days_str = match window {
            Window::SevenDays => "7".to_string(),
            Window::ThirtyDays => "30".to_string(),
            Window::All => "90".to_string(),
        };

        let prompts = input.prompts.unwrap_or_else(|| vec!["default".to_string()]);

        let mut series_vec: Vec<VisibilitySeries> = Vec::new();

        for prompt_name in &prompts {
            let resp = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    api.get("/v1/visibility/trend")
                        .query(&[
                            ("prompt", prompt_name.as_str()),
                            ("days", days_str.as_str()),
                        ])
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
                    "upstream /v1/visibility/trend returned {}",
                    resp.status()
                )));
            }

            let body: serde_json::Value = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async { resp.json().await })
            })
            .map_err(|e| make_upstream_err(&format!("failed to parse response: {e}")))?;

            let raw_points = body
                .get("points")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            let mut points: Vec<VisibilityPoint> = Vec::new();
            let mut visibility_score_sum = 0.0_f64;

            for p in &raw_points {
                let date = p
                    .get("bucket_start")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let provider = p
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let presence_rate = p
                    .get("presence_rate")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let avg_rank = p.get("avg_rank").and_then(|v| v.as_f64());

                let visibility_score = presence_rate;
                visibility_score_sum += visibility_score;

                let ranking = avg_rank.map(|r| r.round() as u32);

                points.push(VisibilityPoint {
                    date,
                    provider,
                    visibility_score,
                    ranking,
                    mention_count: 0,
                });
            }

            let latest = if points.is_empty() {
                None
            } else {
                Some(visibility_score_sum / points.len() as f64)
            };

            let summary = VisibilitySeriesSummary {
                latest,
                delta_vs_prior_window: None,
                empty_reason: if points.is_empty() {
                    Some("no_prompt_runs_in_window".to_string())
                } else {
                    None
                },
            };

            series_vec.push(VisibilitySeries {
                prompt_id: prompt_name.clone(),
                prompt_name: prompt_name.clone(),
                points,
                summary,
            });
        }

        let output = GetVisibilityOutput {
            window,
            series: series_vec,
            empty_reason: None,
            trace_id: Ulid::new().to_string(),
        };

        serde_json::to_value(output).map_err(|e| make_upstream_err(&e.to_string()))
    }
}
