//! MCP JSON-RPC request/response envelopes and error taxonomy.
//!
//! These wrap the per-tool input/output DTOs in [`crate::mcp::tools`]. The
//! envelope itself is wire-stable across all tools; only the `params` and
//! `result` payloads vary.
//!
//! Source: architecture-phase3-mcp-server.md §2 (envelope) + §3.0 (error
//! taxonomy).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::ApiError;

/// Generic MCP `tools/call` request envelope.
///
/// The MCP protocol carries this inside a JSON-RPC 2.0 `params` block;
/// this struct is the typed shape of `params.arguments` for any one tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "McpToolCall")]
pub struct McpToolCall<I> {
    /// MCP tool name — one of the 6 FR-46..FR-51 tools.
    pub name: String,
    /// Tool-specific argument payload (see [`crate::mcp::tools`]).
    pub arguments: I,
}

/// Generic MCP `tools/call` response envelope.
///
/// Per architecture §3.0: success responses set `is_error = false` and carry
/// the typed `result`. Error responses set `is_error = true` and carry an
/// [`McpError`] in `error`. Mutually exclusive — exactly one of `result` /
/// `error` is `Some`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[schemars(rename = "McpToolResponse")]
pub struct McpToolResponse<O> {
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<O>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

/// MCP-level error envelope. Mirrors the REST `ErrorEnvelope` shape
/// (`crates/wire-schema::ErrorEnvelope`) plus MCP-specific kinds.
///
/// See architecture-phase3-mcp-server.md §3.0 error taxonomy table.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct McpError {
    /// Machine-readable error kind.
    #[serde(rename = "type")]
    pub kind: McpErrorKind,
    /// Human-readable, single-line. Never contains secrets.
    pub message: String,
    /// Optional structured detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Correlation ID — ULID. Same value as the response `trace_id`.
    pub request_id: String,
    /// When this error was produced by an underlying `/v1` REST call, the
    /// original [`ApiError`] is embedded verbatim so programmatic consumers
    /// can recover the upstream context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<ApiError>,
}

/// MCP error kind — superset of REST `ProviderErrorKind` + API kinds +
/// MCP-specific kinds (architecture §3.0 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpErrorKind {
    // ---- provider_* — pass-through of crate::ProviderErrorKind ----
    ProviderRateLimited,
    ProviderTimeout,
    ProviderAuthFailed,
    ProviderServerError,
    ProviderInvalidResponse,
    ProviderUnavailable,
    NetworkError,
    // ---- API-level kinds (Phase 2) ----
    ValidationFailed,
    NotFound,
    AuthInvalid,
    InternalError,
    // ---- MCP-specific kinds (Phase 3) ----
    UpstreamUnreachable,
    UpstreamTimeout,
    ToolDisabled,
}
