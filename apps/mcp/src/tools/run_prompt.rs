//! FR-46 `run_prompt` tool stub. Body lands in Story 16.2.

use super::{parse_schema, Tool};

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/run_prompt.input.json");

pub struct RunPrompt;

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
}
