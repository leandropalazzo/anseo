//! Axum extractors shared across `/v1/*` route handlers.
//!
//! Story 0.11 (Phase 3 substrate, decision L2): introduces the
//! `X-OpenGEO-Project` header substrate. Phase 2 single-project mode
//! accepts the header for forward-compatibility; Phase 4 will flip to
//! required-and-resolved without further wire changes.

pub mod project;

pub use project::{project_header_guard, ProjectId, PROJECT_HEADER};
