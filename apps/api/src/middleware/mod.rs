//! Phase 2 axum middleware. Story 12.1 lands `auth`; webhook signing
//! (Story 12.4) and notification dispatch (Story 12.5) will add siblings.
//! Story 20.4 adds `org_guc` for request-scoped org context.

pub mod auth;
pub mod authz;
pub mod geo_gate;
pub mod org_guc;
