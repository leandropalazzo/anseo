//! Canonical wire-format error envelope.
//!
//! Every non-2xx API response and every CLI JSON-output error uses this shape.
//! Per architecture C-3 (single source of truth for HTTP status mapping, CLI
//! exit codes, and MCP tool errors).

use anseo_core::{ProviderErrorKind, RequestId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Outer wrapper. Always `{ "error": { ... } }`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ApiError,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ApiError {
    /// Machine-readable error type. Phase 1 values come from
    /// [`ProviderErrorKind`] (snake_case). Phase 2 will widen this to include
    /// API-level kinds (`validation_failed`, `not_found`, …) without renaming
    /// the field.
    #[serde(rename = "type")]
    pub kind: ProviderErrorKind,

    /// Human-readable, single-line, NEVER includes secrets, API keys, raw
    /// provider response bodies, or stack traces.
    pub message: String,

    /// Optional structured detail. Free-form; consumers should not rely on shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,

    /// ULID correlation ID. Echoed in `X-Anseo-Request-Id` response header
    /// (architecture L632).
    pub request_id: RequestId,
}
