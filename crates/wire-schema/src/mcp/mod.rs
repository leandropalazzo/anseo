//! MCP tool wire-shape DTOs (Story 0.6, Epic 0 substrate for Epic 16 MCP server).
//!
//! Source of truth: `_bmad-output/planning-artifacts/architecture-phase3-mcp-server.md` §3.
//! All shapes here are committed in epic-0 so that the Epic 16 MCP server lands
//! against a frozen wire surface (per RR-Phase3-OpenApiCanonical).
//!
//! ## Conventions
//! - All field names are `snake_case`; no rename layer.
//! - Every input + output struct derives
//!   `Serialize + Deserialize + JsonSchema + ToSchema`.
//! - Every tool input takes `project: ProjectId` (required) per
//!   `AD-Phase3-MCP-ProjectScoping` (§4 of the MCP slice). FR-51
//!   (`search_benchmarks`) is the documented exception — explicitly
//!   project-less.
//! - Every tool output carries `trace_id` so MCP clients can correlate with
//!   `/v1` server logs. (The transport-level `X-Anseo-Request-Id` is the
//!   same value.)
//! - `non_deterministic_pipeline: bool` is surfaced where the architecture
//!   doc calls it out (`run_prompt` results — provider calls are inherently
//!   non-deterministic). Tools whose contract is deterministic (`compare_brands`
//!   — see §3.3 determinism contract) do not carry this flag.
//!
//! ## Project header substrate
//! Per `AD-Phase3-MCP-ProjectScoping` §4, the MCP server forwards
//! `X-Anseo-Project: <project>` on every `/v1` call. The header value type
//! is [`ProjectId`]; the actual header wiring lands in Story 0.11 (apps/api
//! extractor + middleware). This crate only defines the type alias.

pub mod envelope;
pub mod tools;

pub mod json_schema;

pub use envelope::{McpError, McpErrorKind, McpToolCall, McpToolResponse};
pub use tools::*;

/// Identifier for an OpenGEO project, carried in MCP tool inputs and forwarded
/// as the `X-Anseo-Project` header to `/v1` (Story 0.11 wires the header).
///
/// In Phase 3 single-project deployments the server default-resolves this
/// against `AppState.project_name`; in Phase 4 multi-project mode the project
/// is looked up in a registry. See architecture-phase3-mcp-server.md §4.
pub type ProjectId = String;
