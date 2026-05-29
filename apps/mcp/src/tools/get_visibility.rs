//! FR-47 `get_visibility` tool stub. Body lands in Story 16.2.

use super::{parse_schema, Tool};

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/get_visibility.input.json");

pub struct GetVisibility;

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
}
