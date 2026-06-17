//! Story 20.11 — authZ seam (D-P4-8, [p4-iso-4]).
//!
//! Provides the minimal `AuthzDecider` trait and a deny-by-default stub.
//! Story 22.1 replaces the stub body with the full 5-role RBAC matrix.
//!
//! Ordering invariant (AC-1 / AC-2):
//!   1. `AuthzDecider::decide` is called FIRST, before the org GUC is set.
//!   2. Only after an `Ok(Decision::Allow)` is the GUC set via SET LOCAL.
//!   3. A failed `decide` or a `Decision::Deny` returns early — zero DB reads.
//!
//! Fault-injection (AC-3):
//!   If the GUC SET fails after an allowed decision, the transaction must abort
//!   (see `GucContext::set_local` which propagates errors; callers are expected
//!   to rollback on error rather than continue to a data-reading query).

use uuid::Uuid;

/// The outcome of an authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

/// Error type for authorization failures.
#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("authorization store error: {0}")]
    Store(String),
    #[error("GUC set failed: {0}")]
    GucSet(String),
}

/// Minimal seam that the request path calls before setting the org GUC.
///
/// Story 22.1 fills this with the full RBAC matrix. Until then, the
/// `DenyAllDecider` (below) is the default — explicit Allow stubs can be
/// injected in tests.
pub trait AuthzDecider: Send + Sync {
    /// Returns `Allow` if the caller may access the given org, `Deny` otherwise.
    ///
    /// This MUST be called before setting `app.org` — it is the invariant
    /// enforced by the ordering test in `crates/storage/tests/authz_ordering.rs`.
    fn decide(&self, caller_id: Uuid, org_id: Uuid) -> Result<Decision, AuthzError>;
}

/// Deny-by-default stub. Used until Story 22.1 fills the matrix.
#[derive(Debug, Clone, Default)]
pub struct DenyAllDecider;

impl AuthzDecider for DenyAllDecider {
    fn decide(&self, _caller_id: Uuid, _org_id: Uuid) -> Result<Decision, AuthzError> {
        Ok(Decision::Deny)
    }
}

/// Allow-all stub for use in tests that need to reach the GUC-set path.
#[derive(Debug, Clone, Default)]
pub struct AllowAllDecider;

impl AuthzDecider for AllowAllDecider {
    fn decide(&self, _caller_id: Uuid, _org_id: Uuid) -> Result<Decision, AuthzError> {
        Ok(Decision::Allow)
    }
}

/// A decider that errors on `decide` — used to test the authZ-failure path.
#[derive(Debug, Clone, Default)]
pub struct ErrorDecider;

impl AuthzDecider for ErrorDecider {
    fn decide(&self, _caller_id: Uuid, _org_id: Uuid) -> Result<Decision, AuthzError> {
        Err(AuthzError::Store("injected failure".into()))
    }
}

/// The GUC-setting context. Wraps the raw set_config call so it can be
/// fault-injected in tests (AC-3).
pub trait GucContext: Send + Sync {
    /// Set `app.org` to `org_id` using SET LOCAL (transaction-scoped).
    /// Returns `Err` if the set fails; callers MUST rollback on error.
    fn set_local(&self, org_id: Uuid) -> Result<(), AuthzError>;
}

/// Always succeeds. Used in non-fault-injection paths.
#[derive(Debug, Clone, Default)]
pub struct NoopGucContext;

impl GucContext for NoopGucContext {
    fn set_local(&self, _org_id: Uuid) -> Result<(), AuthzError> {
        Ok(())
    }
}

/// Always fails — simulates a `SET LOCAL` that throws.
#[derive(Debug, Clone, Default)]
pub struct FaultyGucContext;

impl GucContext for FaultyGucContext {
    fn set_local(&self, _org_id: Uuid) -> Result<(), AuthzError> {
        Err(AuthzError::GucSet("injected SET LOCAL failure".into()))
    }
}

/// The ordered entry point: authZ THEN GUC.
///
/// Returns `Ok(Decision::Allow)` if both steps succeed, forwarding the Decision
/// so the caller can short-circuit on `Deny` before even attempting `set_local`.
///
/// Invariant: `guc.set_local` is NEVER called when `decider.decide` returns
/// `Deny` or `Err`. This is the AC-1 / AC-2 ordering guarantee.
pub fn authz_then_guc(
    decider: &dyn AuthzDecider,
    guc: &dyn GucContext,
    caller_id: Uuid,
    org_id: Uuid,
) -> Result<Decision, AuthzError> {
    // Step 1: authZ decision (no GUC read — org-independent or org-scoped via
    // a separate privileged role, never via the tenant GUC).
    let decision = decider.decide(caller_id, org_id)?;

    if decision == Decision::Deny {
        return Ok(Decision::Deny);
    }

    // Step 2: set GUC — only after Allow.
    guc.set_local(org_id)?;

    Ok(Decision::Allow)
}
