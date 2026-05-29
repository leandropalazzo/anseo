//! Repository surface for the Phase 1 tables.
//!
//! Each repo borrows a `&PgPool` from [`crate::Storage`] and exposes the
//! minimal `insert` + `get` shape required by Story 1.3 AC-5. Higher-level
//! query methods (lists, filters, joins) land in later stories alongside the
//! features that need them.

pub mod api_keys;
pub mod benchmark_consent;
pub mod citations;
pub mod mentions;
pub mod projects;
pub mod prompt_runs;
pub mod prompts;
pub mod webhook_deliveries;
pub mod webhooks;
