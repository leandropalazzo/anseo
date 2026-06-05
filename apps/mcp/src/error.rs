//! MCP error taxonomy mirror — see `crates/wire-schema/src/mcp/envelope.rs`
//! for the wire shape. This module wraps that taxonomy with a `thiserror`-style
//! enum the dispatch layer can map onto JSON-RPC error codes.

use opengeo_wire_schema::mcp::{McpError, McpErrorKind};

/// Internal error type returned by `Tool::call` handlers.
///
/// Story 16.1 only ever produces `NotImplemented`; later stories (16.2-16.5)
/// will map upstream `/v1` failures into the richer kinds.
/// Story 36.5 adds `UnknownProject` for when a project selector does not match
/// any known project — surfaces as a structured tool error, never a silent
/// fall-through.
#[derive(Debug, Clone)]
pub enum McpToolError {
    /// Stub for stories 16.2-16.5. JSON-RPC code -32601 per Story 16.1 spec.
    NotImplemented,
    /// The project selector named a project that does not exist.
    /// AC 36.5-3: unknown project → structured tool error (not a silent default).
    UnknownProject(String),
    /// Reserved for future use by 16.2+.
    #[allow(dead_code)]
    Upstream(McpError),
}

impl McpToolError {
    /// Convert to a wire-shape MCP error envelope. `trace_id` is the
    /// per-request ULID generated at the dispatch boundary.
    #[allow(dead_code)]
    pub fn into_envelope(self, trace_id: String) -> McpError {
        match self {
            Self::NotImplemented => McpError {
                kind: McpErrorKind::InternalError,
                message: "tool not implemented".into(),
                details: None,
                request_id: trace_id,
                upstream: None,
            },
            Self::UnknownProject(project) => McpError {
                kind: McpErrorKind::InternalError,
                message: format!("unknown project: {project}"),
                details: None,
                request_id: trace_id,
                upstream: None,
            },
            Self::Upstream(err) => err,
        }
    }
}
