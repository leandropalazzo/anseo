//! FR-48 `compare_brands` tool stub. Body lands in Story 16.3.

use super::{parse_schema, Tool};

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/compare_brands.input.json");

pub struct CompareBrands;

impl Tool for CompareBrands {
    fn name(&self) -> &'static str {
        "compare_brands"
    }
    fn description(&self) -> &'static str {
        "Return a deterministic comparison matrix of the configured brand vs. its declared competitors across prompts and providers."
    }
    fn input_schema(&self) -> serde_json::Value {
        parse_schema(INPUT_SCHEMA)
    }
}
