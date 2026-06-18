//! Story 20.11 + 22.1 — authZ seam and RBAC policy module.
//!
//! Story 20.11: the `AuthzDecider` trait + deny-by-default stub (seam).
//! Story 22.1: `matrix` module fills the 5-role RBAC matrix; `RbacDecider`
//!   implements `AuthzDecider` using the matrix.
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

pub mod matrix;

use matrix::Role;
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

/// Story 22.3 — brand-grant scoping policy.
///
/// Owner/Admin implicitly see every brand in the org. Operator/Viewer must have
/// a live `brand_grants` row for the project. Billing has no brand-data access.
/// The DB lookup is deliberately left to callers so grants are checked on every
/// request/list and revocations take effect immediately.
pub fn role_bypasses_brand_grants(role: Role) -> bool {
    matches!(role, Role::Owner | Role::Admin)
}

/// Returns true when this role needs an explicit `brand_grants` row before it
/// may see or operate on a brand.
pub fn role_requires_brand_grant(role: Role) -> bool {
    matches!(role, Role::Operator | Role::Viewer)
}

/// Single policy point for Story 22.3 per-brand access.
pub fn can_access_brand(role: Role, has_grant: bool) -> bool {
    if role_bypasses_brand_grants(role) {
        true
    } else if role_requires_brand_grant(role) {
        has_grant
    } else {
        false
    }
}

/// Story 22.1 — RBAC-backed decider.
///
/// Resolves the caller's role from a user-provided lookup function, then
/// checks the `matrix::is_allowed` table for the requested capability.
///
/// `role_lookup` is a sync closure so it can be implemented against the DB
/// connection pool or a cache without imposing async on this seam.
pub struct RbacDecider<F>
where
    F: Fn(Uuid, Uuid) -> Option<matrix::Role> + Send + Sync,
{
    role_lookup: F,
    capability: matrix::Capability,
}

impl<F> RbacDecider<F>
where
    F: Fn(Uuid, Uuid) -> Option<matrix::Role> + Send + Sync,
{
    /// Create a new `RbacDecider`.
    ///
    /// * `role_lookup(caller_id, org_id)` — returns the caller's role in the org,
    ///   or `None` if they are not a member (→ Deny).
    /// * `capability` — the capability being checked.
    pub fn new(role_lookup: F, capability: matrix::Capability) -> Self {
        Self {
            role_lookup,
            capability,
        }
    }
}

impl<F> AuthzDecider for RbacDecider<F>
where
    F: Fn(Uuid, Uuid) -> Option<matrix::Role> + Send + Sync,
{
    fn decide(&self, caller_id: Uuid, org_id: Uuid) -> Result<Decision, AuthzError> {
        match (self.role_lookup)(caller_id, org_id) {
            None => Ok(Decision::Deny),
            Some(role) => {
                if matrix::is_allowed(role, self.capability) {
                    Ok(Decision::Allow)
                } else {
                    Ok(Decision::Deny)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_and_admin_bypass_brand_grants() {
        assert!(can_access_brand(Role::Owner, false));
        assert!(can_access_brand(Role::Admin, false));
    }

    #[test]
    fn operator_and_viewer_require_live_brand_grant() {
        assert!(!can_access_brand(Role::Operator, false));
        assert!(can_access_brand(Role::Operator, true));
        assert!(!can_access_brand(Role::Viewer, false));
        assert!(can_access_brand(Role::Viewer, true));
    }

    #[test]
    fn billing_never_gets_brand_data_through_grants() {
        assert!(!can_access_brand(Role::Billing, false));
        assert!(!can_access_brand(Role::Billing, true));
    }
}
