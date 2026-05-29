//! FR-50 `list_trends` tool stub. Body lands in Story 16.4.

use super::{parse_schema, Tool};

const INPUT_SCHEMA: &str =
    include_str!("../../../../crates/wire-schema/schemas/mcp/list_trends.input.json");

pub struct ListTrends;

impl Tool for ListTrends {
    fn name(&self) -> &'static str {
        "list_trends"
    }
    fn description(&self) -> &'static str {
        "List threshold regressions, statistical anomalies, and response-change trends detected for a project in a time window."
    }
    fn input_schema(&self) -> serde_json::Value {
        parse_schema(INPUT_SCHEMA)
    }
}
