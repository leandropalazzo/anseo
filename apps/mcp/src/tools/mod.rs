//! MCP tool registry. Each FR-46..FR-51 tool implements [`Tool`].
//!
//! Story 16.1: registry populated, `name()`/`description()`/`input_schema()`
//! return the committed wire-schema snapshots, and `call()` returns
//! `NotImplemented`. Stories 16.2-16.5 land the bodies.

use crate::error::McpToolError;

pub mod compare_brands;
pub mod get_citations;
pub mod get_visibility;
pub mod list_trends;
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
    /// Story 16.1 always returns [`McpToolError::NotImplemented`].
    #[allow(clippy::result_large_err)] // McpToolError grows Upstream variants in 16.2+
    fn call(&self, _args: serde_json::Value) -> Result<serde_json::Value, McpToolError> {
        Err(McpToolError::NotImplemented)
    }
}

/// Build the canonical 6-tool registry. Order matches PRD §6.11 FR-46..FR-51.
pub fn registry() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(run_prompt::RunPrompt),
        Box::new(get_visibility::GetVisibility),
        Box::new(compare_brands::CompareBrands),
        Box::new(get_citations::GetCitations),
        Box::new(list_trends::ListTrends),
        Box::new(search_benchmarks::SearchBenchmarks),
    ]
}

/// Helper: parse a committed JSON Schema snapshot. Panics only on a
/// developer error (corrupt snapshot in-tree) — guarded at startup.
fn parse_schema(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).expect("committed wire-schema snapshot must be valid JSON")
}
