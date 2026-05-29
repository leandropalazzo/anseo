//! JSON-RPC 2.0 framing for the OpenGEO MCP server.
//!
//! Hand-rolled per Phase 3 decision OQ-P3-1 (`_bmad-output/planning-artifacts/
//! phase3-kickoff-decisions-2026-05-29.md`): no `rmcp` dep until both the MCP
//! spec and `rmcp` hit 1.0. This module is pure types + serde — no I/O.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 message id. The spec allows `string | number | null`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Id {
    Num(i64),
    Str(String),
    Null,
}

/// JSON-RPC 2.0 request. `id` absent ⇒ notification (handled separately).
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[serde(rename = "jsonrpc")]
    pub _jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    #[serde(default)]
    pub id: Option<Id>,
}

/// JSON-RPC 2.0 success response.
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Id,
    pub result: serde_json::Value,
}

impl Response {
    pub fn ok(id: Id, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result,
        }
    }
}

/// JSON-RPC 2.0 error response.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub jsonrpc: &'static str,
    pub id: Id,
    pub error: ErrorObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(id: Id, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            error: ErrorObject {
                code,
                message: message.into(),
                data: None,
            },
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.error.data = Some(data);
        self
    }
}

// Standard JSON-RPC error codes.
// Stories 16.2+ exercise the full set as tool handlers gain input validation.
#[allow(dead_code)]
pub const PARSE_ERROR: i32 = -32700;
#[allow(dead_code)]
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
#[allow(dead_code)]
pub const INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
pub const INTERNAL_ERROR: i32 = -32603;
