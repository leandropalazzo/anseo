//! Repository surface for the Phase 1 tables.
//!
//! Each repo borrows a `&PgPool` from [`crate::Storage`] and exposes the
//! minimal `insert` + `get` shape required by Story 1.3 AC-5. Higher-level
//! query methods (lists, filters, joins) land in later stories alongside the
//! features that need them.

pub mod alert_rules;
pub mod anonymous_contributions;
pub mod api_keys;
pub mod audit;
pub mod benchmark_consent;
pub mod benchmark_gate;
pub mod brand_accuracy;
pub mod citations;
pub mod contributions;
pub mod disputes;
pub mod entities;
pub mod mentions;
pub mod org_audit;
pub mod org_branding;
pub mod org_dunning;
pub mod org_entitlements;
pub mod organizations;
pub mod plugin_installs;
pub mod projects;
pub mod prompt_runs;
pub mod prompts;
pub mod recommendations;
pub mod run_provenance;
pub mod site_events;
pub mod verification;
pub mod webhook_deliveries;
pub mod webhooks;
