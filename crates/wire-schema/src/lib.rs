//! Canonical wire DTOs for OpenGEO.
//!
//! The single source of truth for JSON shapes crossing REST, MCP, Webhook,
//! and CLI-JSON boundaries (ARCH C-2).
//!
//! ## Contract
//! - All field names are `snake_case`. No rename layer between Rust and JSON.
//! - All DTOs derive `Serialize + Deserialize + utoipa::ToSchema + schemars::JsonSchema`.
//! - The Provider error taxonomy is re-exported from [`opengeo_core`] to keep
//!   one source of truth for HTTP status mapping, CLI exit codes, and MCP tool errors.

pub mod error;

pub use error::{ApiError, ErrorEnvelope};
pub use opengeo_core::{ProviderErrorKind, RequestId};
