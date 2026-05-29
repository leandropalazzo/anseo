//! FR-49 `get_citations` tool stub. Body lands in Story 16.4.

use super::{parse_schema, Tool};

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/get_citations.input.json");

pub struct GetCitations;

impl Tool for GetCitations {
    fn name(&self) -> &'static str {
        "get_citations"
    }
    fn description(&self) -> &'static str {
        "Return the top-N cited domains for a project within a time window, with frequency, source type, and sample prompt-run IDs."
    }
    fn input_schema(&self) -> serde_json::Value {
        parse_schema(INPUT_SCHEMA)
    }
}
