//! FR-51 `search_benchmarks` tool stub. Body lands in Story 16.5.
//!
//! Project-less per AD-Phase3-MCP-ProjectScoping §4. Underlying handler uses a
//! separate `benchmark_client` (no API key, no project header) when wired in
//! 16.5.

use super::{parse_schema, Tool};

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/search_benchmarks.input.json");

pub struct SearchBenchmarks;

impl Tool for SearchBenchmarks {
    fn name(&self) -> &'static str {
        "search_benchmarks"
    }
    fn description(&self) -> &'static str {
        "Search the OpenGEO public benchmark dataset for category findings. Sends no local-deployment data — query and provider filter only."
    }
    fn input_schema(&self) -> serde_json::Value {
        parse_schema(INPUT_SCHEMA)
    }
}
