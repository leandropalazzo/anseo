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
pub mod ingest_run;
pub mod list_suite_prompts;
pub mod list_trends;
pub mod plugins;
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
        // Story 40.1 — local run ingestion tool.
        Box::new(ingest_run::IngestRun),
        // Story 41.3 — plugin install surface (CLI ⇄ Web ⇄ MCP parity). These
        // are first-party tools compiled into the binary; the registry remains
        // a closed set with no plugin-registration API.
        Box::new(plugins::ListPlugins),
        Box::new(plugins::InstallPlugin),
        Box::new(list_suite_prompts::ListSuitePrompts),
    ]
}

/// Helper: parse a committed JSON Schema snapshot. Panics only on a
/// developer error (corrupt snapshot in-tree) — guarded at startup.
fn parse_schema(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).expect("committed wire-schema snapshot must be valid JSON")
}

/// Story 36.5: map a 404 from the upstream API into an `UnknownProject` error
/// when the request carried an explicit project selector via `X-Anseo-Project`.
///
/// Call this helper after receiving a non-2xx response from the API — it checks
/// whether the status is 404 and returns `UnknownProject` so the dispatcher
/// can surface a structured tool error to the MCP client instead of a generic
/// upstream failure.
pub(crate) fn map_project_not_found(
    status: reqwest::StatusCode,
    api: &ApiClient,
) -> Option<McpToolError> {
    if status == reqwest::StatusCode::NOT_FOUND {
        Some(McpToolError::UnknownProject(
            api.current_project().to_owned(),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_wire_schema::mcp::tools::{TrendDelta, TrendRecord};

    /// Story 17.6 / AD-Phase3-PluginsCannotRegisterMcpTools: the MCP tool set
    /// is a closed, fixed list. The registry is a fixed function with no
    /// registration API, so plugins have no surface to add a tool — the only
    /// way to add one is to edit this binary. This test pins the count and
    /// names so a stray plugin-tool seam can never sneak in. Story 40.1 adds
    /// `ingest_run`; Story 41.3 adds the first-party `list_plugins` +
    /// `install_plugin` tools (operator install parity — still no
    /// plugin-registration seam).
    #[test]
    fn registry_is_the_closed_tool_set() {
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
                "ingest_run",
                "list_plugins",
                "install_plugin",
                "list_suite_prompts",
            ]
        );
    }

    /// Story 17.6 / AD-Phase3-PluginTrendKinds: `trend_kind` is a free-form
    /// string, so a plugin-namespaced kind (`plugin:<name>:<kind>`) surfaces
    /// through `list_trends` verbatim — built-ins stay unprefixed.
    #[test]
    fn trend_record_carries_plugin_namespaced_kind_verbatim() {
        let kind = anseo_plugin_manifest::trend_kind::namespaced_trend_kind(
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
