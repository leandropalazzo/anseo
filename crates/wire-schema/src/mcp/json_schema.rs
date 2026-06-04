//! JSON Schema generation for MCP tool DTOs.
//!
//! The output of [`all_schemas`] feeds MCP `tools/list` `inputSchema` /
//! `outputSchema` and a drift test in `tests/mcp_schema_drift.rs` asserts
//! byte-identity against committed snapshots under `schemas/mcp/`.
//!
//! Schema files are pretty-printed with `serde_json::to_string_pretty` and
//! terminated with a single trailing newline so they diff cleanly.

use schemars::{schema::RootSchema, schema_for};

use crate::mcp::tools::*;

/// One entry per committed schema file.
pub struct McpSchemaEntry {
    /// File name under `crates/wire-schema/schemas/mcp/` (without extension).
    pub name: &'static str,
    /// Pretty-printed JSON Schema text (with trailing newline).
    pub schema_json: String,
}

/// All 22 MCP schemas — 6 Phase-2 tools + 5 Story-19.7 `recommend.*` tools,
/// each × input/output.
pub fn all_schemas() -> Vec<McpSchemaEntry> {
    vec![
        entry("run_prompt.input", schema_for!(RunPromptInput)),
        entry("run_prompt.output", schema_for!(RunPromptOutput)),
        entry("get_visibility.input", schema_for!(GetVisibilityInput)),
        entry("get_visibility.output", schema_for!(GetVisibilityOutput)),
        entry("compare_brands.input", schema_for!(CompareBrandsInput)),
        entry("compare_brands.output", schema_for!(CompareBrandsOutput)),
        entry("get_citations.input", schema_for!(GetCitationsInput)),
        entry("get_citations.output", schema_for!(GetCitationsOutput)),
        entry("list_trends.input", schema_for!(ListTrendsInput)),
        entry("list_trends.output", schema_for!(ListTrendsOutput)),
        entry(
            "search_benchmarks.input",
            schema_for!(SearchBenchmarksInput),
        ),
        entry(
            "search_benchmarks.output",
            schema_for!(SearchBenchmarksOutput),
        ),
        entry("recommend.list.input", schema_for!(RecommendListInput)),
        entry("recommend.list.output", schema_for!(RecommendListOutput)),
        entry("recommend.show.input", schema_for!(RecommendShowInput)),
        entry("recommend.show.output", schema_for!(RecommendShowOutput)),
        entry("recommend.ack.input", schema_for!(RecommendAckInput)),
        entry(
            "recommend.ack.output",
            schema_for!(RecommendTransitionOutput),
        ),
        entry(
            "recommend.dismiss.input",
            schema_for!(RecommendDismissInput),
        ),
        entry(
            "recommend.dismiss.output",
            schema_for!(RecommendTransitionOutput),
        ),
        entry(
            "recommend.mark_acted.input",
            schema_for!(RecommendMarkActedInput),
        ),
        entry(
            "recommend.mark_acted.output",
            schema_for!(RecommendTransitionOutput),
        ),
        entry("audit.input", schema_for!(AuditInput)),
        entry("audit.output", schema_for!(AuditOutput)),
    ]
}

fn entry(name: &'static str, schema: RootSchema) -> McpSchemaEntry {
    let mut s = serde_json::to_string_pretty(&schema).expect("schema serializes");
    s.push('\n');
    McpSchemaEntry {
        name,
        schema_json: s,
    }
}
