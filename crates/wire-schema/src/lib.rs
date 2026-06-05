//! Canonical wire DTOs for OpenGEO.
//!
//! The single source of truth for JSON shapes crossing REST, MCP, Webhook,
//! and CLI-JSON boundaries (ARCH C-2).
//!
//! ## Contract
//! - All field names are `snake_case`. No rename layer between Rust and JSON.
//! - All DTOs derive `Serialize + Deserialize + utoipa::ToSchema + schemars::JsonSchema`.
//! - The Provider error taxonomy is re-exported from [`anseo_core`] to keep
//!   one source of truth for HTTP status mapping, CLI exit codes, and MCP tool errors.

// Note: `Unauthorized` $ref is declared above but its component lives at
// `components.responses` (canonical OpenAPI); the gen-openapi binary will
// surface that block once a Phase-3 utoipa migration lands. For now the
// $ref is intentional documentation that downstream codegen consumes.

pub mod error;
pub mod mcp;
pub mod parity;
pub mod webhook;

pub use anseo_core::{ProviderErrorKind, RequestId};
pub use error::{ApiError, ErrorEnvelope};
