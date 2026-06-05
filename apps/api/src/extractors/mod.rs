//! Axum extractors shared across `/v1/*` route handlers.
//!
//! Epic 36 (ADR-004): `project` provides per-request project resolution over
//! the `projects` table. The `X-Anseo-Project` header (or an explicit
//! CLI/MCP flag) resolves to a concrete `ProjectId`; the resolved scope is
//! stamped into request extensions for every `/v1/*` handler.

pub mod project;

pub use project::{
    project_header_guard, resolve_project, EffectiveProject, ProjectId, ProjectScope, ResolveError,
    PROJECT_HEADER,
};
