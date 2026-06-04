//! MCP tool registry. Each FR-46..FR-51 tool implements [`Tool`].
//!
//! Story 16.1: registry populated, `name()`/`description()`/`input_schema()`
//! return the committed wire-schema snapshots, and `call()` returns
//! `NotImplemented`. Stories 16.2-16.5 land the bodies.

use crate::error::McpToolError;
use crate::http_client::ApiClient;

pub mod audit;
pub mod compare_brands;
pub mod get_citations;
pub mod get_visibility;
pub mod list_trends;
pub mod recommend;
pub mod run_prompt;
pub mod search_benchmarks;

/// A single MCP tool.
///
/// The trait is intentionally narrow: the dispatcher only ever needs name,
/// description, an input JSON Schema, and a call-with-args entry point.
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    /// Pre-parsed input schema from the committed wire-schema snapshot.
    /// We deserialize once at startup so `tools/list` is a cheap clone.
    fn input_schema(&self) -> serde_json::Value;
    /// Stories 16.1 stubs return [`McpToolError::NotImplemented`];
    /// 16.2-16.5 land the real bodies.
    #[allow(clippy::result_large_err)]
    fn call(
        &self,
        _args: serde_json::Value,
        _api: &ApiClient,
    ) -> Result<serde_json::Value, McpToolError> {
        Err(McpToolError::NotImplemented)
    }
}

/// Build the canonical 12-tool registry. The first six match PRD §6.11
/// FR-46..FR-51; the next five are the Story 19.7 `recommend.*` tools; the
/// last is the Roadmap Epic-32 `audit` tool. The set is still closed — there
/// is no registration API, so plugins cannot add tools.
pub fn registry() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(run_prompt::RunPrompt),
        Box::new(get_visibility::GetVisibility),
        Box::new(compare_brands::CompareBrands),
        Box::new(get_citations::GetCitations),
        Box::new(list_trends::ListTrends),
        Box::new(search_benchmarks::SearchBenchmarks),
        Box::new(recommend::RecommendList),
        Box::new(recommend::RecommendShow),
        Box::new(recommend::RecommendAck),
        Box::new(recommend::RecommendDismiss),
        Box::new(recommend::RecommendMarkActed),
        Box::new(audit::Audit),
    ]
}

/// Helper: parse a committed JSON Schema snapshot. Panics only on a
/// developer error (corrupt snapshot in-tree) — guarded at startup.
fn parse_schema(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).expect("committed wire-schema snapshot must be valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengeo_wire_schema::mcp::tools::{TrendDelta, TrendRecord};

    /// Story 17.6 / AD-Phase3-PluginsCannotRegisterMcpTools: the MCP tool set
    /// is the closed FR-46..FR-51 list. The registry is a fixed function with
    /// no registration API, so plugins have no surface to add a tool — the
    /// only way to add one is to edit this binary. This test pins the count
    /// and names so a stray plugin-tool seam can never sneak in.
    #[test]
    fn registry_is_the_closed_twelve_tool_set() {
        let names: Vec<&str> = registry().iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            vec![
                "run_prompt",
                "get_visibility",
                "compare_brands",
                "get_citations",
                "list_trends",
                "search_benchmarks",
                "recommend.list",
                "recommend.show",
                "recommend.ack",
                "recommend.dismiss",
                "recommend.mark_acted",
                "audit",
            ]
        );
    }

    /// Story 17.6 / AD-Phase3-PluginTrendKinds: `trend_kind` is a free-form
    /// string, so a plugin-namespaced kind (`plugin:<name>:<kind>`) surfaces
    /// through `list_trends` verbatim — built-ins stay unprefixed.
    #[test]
    fn trend_record_carries_plugin_namespaced_kind_verbatim() {
        let kind = opengeo_plugin_manifest::trend_kind::namespaced_trend_kind(
            "test.analytics",
            "churn_spike",
        );
        let record = TrendRecord {
            trend_kind: kind.clone(),
            prompt_id: String::new(),
            prompt_name: String::new(),
            provider: "openai".to_string(),
            delta: TrendDelta {
                metric: String::new(),
                from: 0.0,
                to: 0.0,
            },
            evidence_prompt_run_ids: vec![],
            significance: 1.0,
            detected_at: String::new(),
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["trend_kind"], "plugin:test.analytics:churn_spike");
    }
}
