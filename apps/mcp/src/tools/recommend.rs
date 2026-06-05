//! Story 19.7 — `recommend.*` MCP tools.
//!
//! Five tools front the Epic 19 REST surface over loopback HTTP per
//! AD-Phase3-MCP-Process-Model: list / show / ack / dismiss / mark_acted.
//! All consume the engine wire envelope verbatim (opaque `serde_json::Value`).
//!
//! Per architecture-phase3-geo-recommendations §5.2 / §8.4, every tool
//! description carries the `non_deterministic_pipeline` best-effort sentence;
//! `recommend.mark_acted` additionally documents the tagging semantics
//! (UX-DR110 / decision L4) and exposes optional `evidence_url` + `note`.

use super::{parse_schema, Tool};
use crate::error::McpToolError;
use crate::http_client::ApiClient;
use anseo_wire_schema::mcp::tools::{
    RecommendAckInput, RecommendDismissInput, RecommendListInput, RecommendListOutput,
    RecommendMarkActedInput, RecommendShowInput, RecommendShowOutput, RecommendTransitionOutput,
};
use ulid::Ulid;

#[cfg(test)]
const NON_DET_SENTENCE: &str =
    "Some recommendations are tagged `non_deterministic_pipeline` and should be treated as best-effort.";

const LIST_INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/recommend.list.input.json");
const SHOW_INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/recommend.show.input.json");
const ACK_INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/recommend.ack.input.json");
const DISMISS_INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/recommend.dismiss.input.json");
const MARK_ACTED_INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/recommend.mark_acted.input.json");

fn upstream_err(msg: &str) -> McpToolError {
    McpToolError::Upstream(anseo_wire_schema::mcp::McpError {
        kind: anseo_wire_schema::mcp::McpErrorKind::InternalError,
        message: msg.to_string(),
        details: None,
        request_id: Ulid::new().to_string(),
        upstream: None,
    })
}

/// Run an async future on the current multi-threaded runtime from a sync
/// `Tool::call` context (mirrors the Phase-2 tool bodies).
fn block<F: std::future::Future>(fut: F) -> F::Output {
    tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
}

// ---- recommend.list -----------------------------------------------------

pub struct RecommendList;

impl Tool for RecommendList {
    fn name(&self) -> &'static str {
        "recommend.list"
    }

    fn description(&self) -> &'static str {
        concat!(
            "List active GEO recommendations for a project, newest first, with cursor pagination. ",
            "Some recommendations are tagged `non_deterministic_pipeline` and should be treated as best-effort."
        )
    }

    fn input_schema(&self) -> serde_json::Value {
        parse_schema(LIST_INPUT_SCHEMA)
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let input = serde_json::from_value::<RecommendListInput>(args)
            .map_err(|e| upstream_err(&format!("invalid params: {e}")))?;

        let mut query: Vec<(String, String)> = Vec::new();
        if let Some(limit) = input.limit {
            query.push(("limit".to_string(), limit.to_string()));
        }
        if let Some(cursor) = input.cursor {
            query.push(("cursor".to_string(), cursor));
        }

        let resp = block(async { api.get("/v1/recommendations").query(&query).send().await })
            .map_err(|e| upstream_err(&e.to_string()))?;

        if !resp.status().is_success() {
            // Story 36.5: 404 from a project-scoped call → UnknownProject.
            if let Some(e) = super::map_project_not_found(resp.status(), api) {
                return Err(e);
            }
            return Err(upstream_err(&format!(
                "upstream /v1/recommendations returned {}",
                resp.status()
            )));
        }

        let body: serde_json::Value = block(async { resp.json().await })
            .map_err(|e| upstream_err(&format!("failed to parse response: {e}")))?;

        let recommendations = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let next_cursor = body
            .get("next_cursor")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let output = RecommendListOutput {
            recommendations,
            next_cursor,
            trace_id: Ulid::new().to_string(),
        };
        serde_json::to_value(output).map_err(|e| upstream_err(&e.to_string()))
    }
}

// ---- recommend.show -----------------------------------------------------

pub struct RecommendShow;

impl Tool for RecommendShow {
    fn name(&self) -> &'static str {
        "recommend.show"
    }

    fn description(&self) -> &'static str {
        concat!(
            "Fetch one recommendation by id, including full traceability and reproducibility class. ",
            "Some recommendations are tagged `non_deterministic_pipeline` and should be treated as best-effort."
        )
    }

    fn input_schema(&self) -> serde_json::Value {
        parse_schema(SHOW_INPUT_SCHEMA)
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let input = serde_json::from_value::<RecommendShowInput>(args)
            .map_err(|e| upstream_err(&format!("invalid params: {e}")))?;

        let path = format!("/v1/recommendations/{}", input.recommendation_id);
        let resp = block(async { api.get(&path).send().await })
            .map_err(|e| upstream_err(&e.to_string()))?;

        if !resp.status().is_success() {
            // Story 36.5: 404 from a project-scoped call → UnknownProject.
            if let Some(e) = super::map_project_not_found(resp.status(), api) {
                return Err(e);
            }
            return Err(upstream_err(&format!(
                "upstream {path} returned {}",
                resp.status()
            )));
        }

        let recommendation: serde_json::Value = block(async { resp.json().await })
            .map_err(|e| upstream_err(&format!("failed to parse response: {e}")))?;

        let output = RecommendShowOutput {
            recommendation,
            trace_id: Ulid::new().to_string(),
        };
        serde_json::to_value(output).map_err(|e| upstream_err(&e.to_string()))
    }
}

// ---- transition helper (ack / dismiss / mark_acted) ---------------------

#[allow(clippy::result_large_err)]
fn transition(
    api: &ApiClient,
    recommendation_id: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, McpToolError> {
    let path = format!("/v1/recommendations/{recommendation_id}/state");
    let resp = block(async { api.patch(&path).json(&body).send().await })
        .map_err(|e| upstream_err(&e.to_string()))?;

    if !resp.status().is_success() {
        // Story 36.5: 404 from a project-scoped call → UnknownProject.
        if let Some(e) = super::map_project_not_found(resp.status(), api) {
            return Err(e);
        }
        return Err(upstream_err(&format!(
            "upstream {path} returned {}",
            resp.status()
        )));
    }

    let parsed: serde_json::Value = block(async { resp.json().await })
        .map_err(|e| upstream_err(&format!("failed to parse response: {e}")))?;

    let recommendation = parsed
        .get("recommendation")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let warnings = parsed
        .get("warnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let output = RecommendTransitionOutput {
        recommendation,
        warnings,
        trace_id: Ulid::new().to_string(),
    };
    serde_json::to_value(output).map_err(|e| upstream_err(&e.to_string()))
}

// ---- recommend.ack ------------------------------------------------------

pub struct RecommendAck;

impl Tool for RecommendAck {
    fn name(&self) -> &'static str {
        "recommend.ack"
    }

    fn description(&self) -> &'static str {
        concat!(
            "Acknowledge a surfaced recommendation (Surfaced -> Acknowledged). ",
            "Some recommendations are tagged `non_deterministic_pipeline` and should be treated as best-effort."
        )
    }

    fn input_schema(&self) -> serde_json::Value {
        parse_schema(ACK_INPUT_SCHEMA)
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let input = serde_json::from_value::<RecommendAckInput>(args)
            .map_err(|e| upstream_err(&format!("invalid params: {e}")))?;
        transition(
            api,
            &input.recommendation_id,
            serde_json::json!({ "to": "acknowledged" }),
        )
    }
}

// ---- recommend.dismiss --------------------------------------------------

pub struct RecommendDismiss;

impl Tool for RecommendDismiss {
    fn name(&self) -> &'static str {
        "recommend.dismiss"
    }

    fn description(&self) -> &'static str {
        concat!(
            "Dismiss a recommendation (-> Dismissed); valid from Surfaced or Acknowledged. ",
            "Some recommendations are tagged `non_deterministic_pipeline` and should be treated as best-effort."
        )
    }

    fn input_schema(&self) -> serde_json::Value {
        parse_schema(DISMISS_INPUT_SCHEMA)
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let input = serde_json::from_value::<RecommendDismissInput>(args)
            .map_err(|e| upstream_err(&format!("invalid params: {e}")))?;
        transition(
            api,
            &input.recommendation_id,
            serde_json::json!({ "to": "dismissed" }),
        )
    }
}

// ---- recommend.mark_acted -----------------------------------------------

pub struct RecommendMarkActed;

impl Tool for RecommendMarkActed {
    fn name(&self) -> &'static str {
        "recommend.mark_acted"
    }

    fn description(&self) -> &'static str {
        concat!(
            "Mark a recommendation as Acted (Acknowledged -> Acted), optionally attaching ",
            "`evidence_url` and a `note`; a `lifecycle.evidence_missing` warning is returned when ",
            "no evidence is supplied. Recommendations tagged `non_deterministic_pipeline` are ",
            "best-effort: the acted outcome may not be reproducible across runs, so evidence is ",
            "especially encouraged for those. ",
            "Some recommendations are tagged `non_deterministic_pipeline` and should be treated as best-effort."
        )
    }

    fn input_schema(&self) -> serde_json::Value {
        parse_schema(MARK_ACTED_INPUT_SCHEMA)
    }

    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        args: serde_json::Value,
        api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        let input = serde_json::from_value::<RecommendMarkActedInput>(args)
            .map_err(|e| upstream_err(&format!("invalid params: {e}")))?;

        let mut body = serde_json::json!({ "to": "acted" });
        if let Some(url) = input.evidence_url {
            body["evidence_url"] = serde_json::Value::String(url);
        }
        if let Some(note) = input.note {
            body["note"] = serde_json::Value::String(note);
        }
        transition(api, &input.recommendation_id, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §5.2/§8.4 binding: every `recommend.*` description carries the
    /// `non_deterministic_pipeline` best-effort sentence verbatim.
    #[test]
    fn all_descriptions_carry_non_deterministic_sentence() {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(RecommendList),
            Box::new(RecommendShow),
            Box::new(RecommendAck),
            Box::new(RecommendDismiss),
            Box::new(RecommendMarkActed),
        ];
        for t in &tools {
            assert!(
                t.description().contains(NON_DET_SENTENCE),
                "{} description missing non_deterministic_pipeline sentence",
                t.name()
            );
        }
    }

    /// Decision L4 / UX-DR110: mark_acted documents the tagging semantics.
    #[test]
    fn mark_acted_documents_tagging_semantics() {
        let d = RecommendMarkActed.description();
        assert!(d.contains("non_deterministic_pipeline"));
        assert!(d.contains("evidence"));
    }

    /// Story 19.9 / UX-DR126 — cross-surface contract: `recommend.list`
    /// re-exposes the REST `/v1/recommendations` envelope verbatim. The same
    /// recommendation Value that the dashboard renders in the Overview tile
    /// and the `/recommendations` list must appear byte-identically in the MCP
    /// tool output, with no field mutation. This mirrors the extraction in
    /// `RecommendList::call` (body["items"] → output.recommendations).
    #[test]
    fn list_passes_rest_envelope_through_byte_identically() {
        // A representative wire envelope as produced by row_to_json.
        let rec = serde_json::json!({
            "id": "01JABCDEF0123456789ABCDEFG",
            "project_id": "01PROJECT0000000000000000",
            "kind": "visibility_gap",
            "severity": "high",
            "confidence_band": "medium",
            "state": "surfaced",
            "summary": "Brand visibility dropped on Perplexity",
            "payload": {},
            "traceability": {
                "source_run_ids": ["01RUN0000000000000000000AA"],
                "source_run_ids_truncated": false,
                "source_citation_ids": [],
                "source_citation_ids_truncated": false,
                "source_benchmark_queries": [],
                "window": {"start": "2026-05-01T00:00:00Z", "end": "2026-05-30T00:00:00Z"},
                "input_fingerprint": "abc123"
            },
            "reproducibility": {"class": "non_deterministic", "note": null},
            "tags": ["non_deterministic_pipeline"],
            "generated_at": "2026-05-30T00:00:00Z",
            "engine_version": "sm14-1.0.0"
        });
        let body = serde_json::json!({ "items": [rec.clone()], "next_cursor": null });

        // Mirror RecommendList::call extraction.
        let recommendations = body
            .get("items")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let output = RecommendListOutput {
            recommendations,
            next_cursor: None,
            trace_id: Ulid::new().to_string(),
        };

        let serialized = serde_json::to_value(&output).unwrap();
        // The recommendation object is byte-identical to the REST envelope item.
        assert_eq!(serialized["recommendations"][0], rec);
    }
}
