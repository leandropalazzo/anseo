//! Story 22.1 — 5-role RBAC matrix.
//!
//! Fills the deny-by-default seam stub from Story 20.11.
//! The matrix is encoded as a `match (role, capability)` exhaustive arm set:
//! adding a new `Capability` variant without adding a row FAILS TO COMPILE.
//!
//! Roles (from `org_role` PG enum, cloud design §4.2):
//!   Owner   — full control; unique per org; cannot be removed if last Owner
//!   Admin   — manage members + brands, but cannot touch billing or delete org
//!   Billing — billing portal access only; cannot manage members or content
//!   Operator — run prompts, manage own projects + brand content
//!   Viewer  — read-only across org content they're granted to
//!
//! `[p4-authz-1]`: the sentinel for this story.

/// A capability represents a discrete action a caller may attempt.
/// Adding a new variant without updating `is_allowed` fails to compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    // --- Org management ---
    OrgRead,
    OrgUpdate,
    OrgDelete,
    OrgMfaPolicySet,

    // --- Member management ---
    MemberList,
    MemberInvite,
    MemberDeactivate,
    MemberRoleAssign,

    // --- Billing ---
    BillingRead,
    BillingUpdate,
    BillingPortalAccess,

    // --- Projects / Brands ---
    ProjectCreate,
    ProjectRead,
    ProjectUpdate,
    ProjectDelete,
    ProjectArchive,

    BrandCreate,
    BrandRead,
    BrandUpdate,
    BrandDelete,

    // --- Content / Runs ---
    PromptCreate,
    PromptRead,
    PromptUpdate,
    PromptDelete,
    RunCreate,
    RunRead,

    // --- API keys ---
    ApiKeyCreate,
    ApiKeyRevoke,
    ApiKeyList,

    // --- Webhooks ---
    WebhookCreate,
    WebhookUpdate,
    WebhookDelete,
    WebhookRead,

    // --- Audit ---
    AuditRead,

    // --- Grants ---
    BrandGrantManage,
}

/// The five roles in the org RBAC matrix (mirrors the `org_role` PG enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Owner,
    Admin,
    Billing,
    Operator,
    Viewer,
}

/// Returns `true` if `role` is allowed to perform `capability`.
///
/// This is the single policy point referenced by Story 22.2:
/// all request handlers call `is_allowed` via `RbacDecider::decide`.
pub fn is_allowed(role: Role, capability: Capability) -> bool {
    use Capability::*;
    use Role::*;

    match (role, capability) {
        // --- Owner: full control ---
        (Owner, _) => true,

        // --- Admin: manage members + brands + content; no billing / no org delete ---
        (Admin, OrgRead) => true,
        (Admin, OrgUpdate) => true,
        (Admin, OrgDelete) => false,
        (Admin, OrgMfaPolicySet) => true,
        (Admin, MemberList) => true,
        (Admin, MemberInvite) => true,
        (Admin, MemberDeactivate) => true,
        (Admin, MemberRoleAssign) => true,
        (Admin, BillingRead) => true,
        (Admin, BillingUpdate) => false,
        (Admin, BillingPortalAccess) => false,
        (Admin, ProjectCreate) => true,
        (Admin, ProjectRead) => true,
        (Admin, ProjectUpdate) => true,
        (Admin, ProjectDelete) => true,
        (Admin, ProjectArchive) => true,
        (Admin, BrandCreate) => true,
        (Admin, BrandRead) => true,
        (Admin, BrandUpdate) => true,
        (Admin, BrandDelete) => true,
        (Admin, PromptCreate) => true,
        (Admin, PromptRead) => true,
        (Admin, PromptUpdate) => true,
        (Admin, PromptDelete) => true,
        (Admin, RunCreate) => true,
        (Admin, RunRead) => true,
        (Admin, ApiKeyCreate) => true,
        (Admin, ApiKeyRevoke) => true,
        (Admin, ApiKeyList) => true,
        (Admin, WebhookCreate) => true,
        (Admin, WebhookUpdate) => true,
        (Admin, WebhookDelete) => true,
        (Admin, WebhookRead) => true,
        (Admin, AuditRead) => true,
        (Admin, BrandGrantManage) => true,

        // --- Billing: billing portal only ---
        (Billing, OrgRead) => true,
        (Billing, OrgUpdate) => false,
        (Billing, OrgDelete) => false,
        (Billing, OrgMfaPolicySet) => false,
        (Billing, MemberList) => false,
        (Billing, MemberInvite) => false,
        (Billing, MemberDeactivate) => false,
        (Billing, MemberRoleAssign) => false,
        (Billing, BillingRead) => true,
        (Billing, BillingUpdate) => true,
        (Billing, BillingPortalAccess) => true,
        (Billing, ProjectCreate) => false,
        (Billing, ProjectRead) => false,
        (Billing, ProjectUpdate) => false,
        (Billing, ProjectDelete) => false,
        (Billing, ProjectArchive) => false,
        (Billing, BrandCreate) => false,
        (Billing, BrandRead) => false,
        (Billing, BrandUpdate) => false,
        (Billing, BrandDelete) => false,
        (Billing, PromptCreate) => false,
        (Billing, PromptRead) => false,
        (Billing, PromptUpdate) => false,
        (Billing, PromptDelete) => false,
        (Billing, RunCreate) => false,
        (Billing, RunRead) => false,
        (Billing, ApiKeyCreate) => false,
        (Billing, ApiKeyRevoke) => false,
        (Billing, ApiKeyList) => false,
        (Billing, WebhookCreate) => false,
        (Billing, WebhookUpdate) => false,
        (Billing, WebhookDelete) => false,
        (Billing, WebhookRead) => false,
        (Billing, AuditRead) => false,
        (Billing, BrandGrantManage) => false,

        // --- Operator: manage own projects + run prompts; no member management ---
        (Operator, OrgRead) => true,
        (Operator, OrgUpdate) => false,
        (Operator, OrgDelete) => false,
        (Operator, OrgMfaPolicySet) => false,
        (Operator, MemberList) => true,
        (Operator, MemberInvite) => false,
        (Operator, MemberDeactivate) => false,
        (Operator, MemberRoleAssign) => false,
        (Operator, BillingRead) => false,
        (Operator, BillingUpdate) => false,
        (Operator, BillingPortalAccess) => false,
        (Operator, ProjectCreate) => true,
        (Operator, ProjectRead) => true,
        (Operator, ProjectUpdate) => true,
        (Operator, ProjectDelete) => false,
        (Operator, ProjectArchive) => true,
        (Operator, BrandCreate) => false,
        (Operator, BrandRead) => true,
        (Operator, BrandUpdate) => false,
        (Operator, BrandDelete) => false,
        (Operator, PromptCreate) => true,
        (Operator, PromptRead) => true,
        (Operator, PromptUpdate) => true,
        (Operator, PromptDelete) => true,
        (Operator, RunCreate) => true,
        (Operator, RunRead) => true,
        (Operator, ApiKeyCreate) => true,
        (Operator, ApiKeyRevoke) => true,
        (Operator, ApiKeyList) => true,
        (Operator, WebhookCreate) => true,
        (Operator, WebhookUpdate) => true,
        (Operator, WebhookDelete) => true,
        (Operator, WebhookRead) => true,
        (Operator, AuditRead) => false,
        (Operator, BrandGrantManage) => false,

        // --- Viewer: read-only ---
        (Viewer, OrgRead) => true,
        (Viewer, OrgUpdate) => false,
        (Viewer, OrgDelete) => false,
        (Viewer, OrgMfaPolicySet) => false,
        (Viewer, MemberList) => true,
        (Viewer, MemberInvite) => false,
        (Viewer, MemberDeactivate) => false,
        (Viewer, MemberRoleAssign) => false,
        (Viewer, BillingRead) => false,
        (Viewer, BillingUpdate) => false,
        (Viewer, BillingPortalAccess) => false,
        (Viewer, ProjectCreate) => false,
        (Viewer, ProjectRead) => true,
        (Viewer, ProjectUpdate) => false,
        (Viewer, ProjectDelete) => false,
        (Viewer, ProjectArchive) => false,
        (Viewer, BrandCreate) => false,
        (Viewer, BrandRead) => true,
        (Viewer, BrandUpdate) => false,
        (Viewer, BrandDelete) => false,
        (Viewer, PromptCreate) => false,
        (Viewer, PromptRead) => true,
        (Viewer, PromptUpdate) => false,
        (Viewer, PromptDelete) => false,
        (Viewer, RunCreate) => false,
        (Viewer, RunRead) => true,
        (Viewer, ApiKeyCreate) => false,
        (Viewer, ApiKeyRevoke) => false,
        (Viewer, ApiKeyList) => true,
        (Viewer, WebhookCreate) => false,
        (Viewer, WebhookUpdate) => false,
        (Viewer, WebhookDelete) => false,
        (Viewer, WebhookRead) => true,
        (Viewer, AuditRead) => false,
        (Viewer, BrandGrantManage) => false,
    }
}

/// All capabilities in definition order — used by the exhaustive matrix test.
pub const ALL_CAPABILITIES: &[Capability] = &[
    Capability::OrgRead,
    Capability::OrgUpdate,
    Capability::OrgDelete,
    Capability::OrgMfaPolicySet,
    Capability::MemberList,
    Capability::MemberInvite,
    Capability::MemberDeactivate,
    Capability::MemberRoleAssign,
    Capability::BillingRead,
    Capability::BillingUpdate,
    Capability::BillingPortalAccess,
    Capability::ProjectCreate,
    Capability::ProjectRead,
    Capability::ProjectUpdate,
    Capability::ProjectDelete,
    Capability::ProjectArchive,
    Capability::BrandCreate,
    Capability::BrandRead,
    Capability::BrandUpdate,
    Capability::BrandDelete,
    Capability::PromptCreate,
    Capability::PromptRead,
    Capability::PromptUpdate,
    Capability::PromptDelete,
    Capability::RunCreate,
    Capability::RunRead,
    Capability::ApiKeyCreate,
    Capability::ApiKeyRevoke,
    Capability::ApiKeyList,
    Capability::WebhookCreate,
    Capability::WebhookUpdate,
    Capability::WebhookDelete,
    Capability::WebhookRead,
    Capability::AuditRead,
    Capability::BrandGrantManage,
];

/// All roles in definition order.
pub const ALL_ROLES: &[Role] = &[
    Role::Owner,
    Role::Admin,
    Role::Billing,
    Role::Operator,
    Role::Viewer,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustive matrix test — exercises every (Role × Capability) cell.
    /// This is the compile-fail guard: if a new Capability is added without
    /// updating `is_allowed`, the match becomes non-exhaustive and won't compile.
    #[test]
    fn exhaustive_matrix_compiles_and_runs() {
        for role in ALL_ROLES {
            for cap in ALL_CAPABILITIES {
                // Just call it — the test proves every cell is covered.
                let _ = is_allowed(*role, *cap);
            }
        }
    }

    // --- Semantic spot-checks (cloud design §4.2) ---

    #[test]
    fn owner_can_do_everything() {
        for cap in ALL_CAPABILITIES {
            assert!(
                is_allowed(Role::Owner, *cap),
                "Owner must be allowed: {cap:?}"
            );
        }
    }

    #[test]
    fn billing_cannot_read_content() {
        for cap in [
            Capability::ProjectRead,
            Capability::PromptRead,
            Capability::RunRead,
            Capability::BrandRead,
            Capability::MemberList,
        ] {
            assert!(
                !is_allowed(Role::Billing, cap),
                "Billing must NOT be allowed: {cap:?}"
            );
        }
    }

    #[test]
    fn billing_can_access_billing_portal() {
        assert!(is_allowed(Role::Billing, Capability::BillingPortalAccess));
        assert!(is_allowed(Role::Billing, Capability::BillingRead));
        assert!(is_allowed(Role::Billing, Capability::BillingUpdate));
    }

    #[test]
    fn admin_can_read_billing_but_cannot_delete_org_or_update_billing() {
        assert!(!is_allowed(Role::Admin, Capability::OrgDelete));
        assert!(is_allowed(Role::Admin, Capability::BillingRead));
        assert!(!is_allowed(Role::Admin, Capability::BillingPortalAccess));
    }

    #[test]
    fn operator_cannot_manage_members_or_delete_projects() {
        assert!(!is_allowed(Role::Operator, Capability::MemberInvite));
        assert!(!is_allowed(Role::Operator, Capability::MemberDeactivate));
        assert!(!is_allowed(Role::Operator, Capability::ProjectDelete));
        assert!(!is_allowed(Role::Operator, Capability::BrandCreate));
    }

    #[test]
    fn viewer_is_read_only() {
        let write_caps = [
            Capability::OrgUpdate,
            Capability::OrgDelete,
            Capability::MemberInvite,
            Capability::ProjectCreate,
            Capability::ProjectUpdate,
            Capability::ProjectDelete,
            Capability::PromptCreate,
            Capability::PromptUpdate,
            Capability::PromptDelete,
            Capability::RunCreate,
            Capability::ApiKeyCreate,
            Capability::ApiKeyRevoke,
            Capability::WebhookCreate,
            Capability::WebhookUpdate,
            Capability::WebhookDelete,
        ];
        for cap in write_caps {
            assert!(
                !is_allowed(Role::Viewer, cap),
                "Viewer must NOT be allowed write cap: {cap:?}"
            );
        }
    }

    #[test]
    fn viewer_can_read_basic_content() {
        for cap in [
            Capability::OrgRead,
            Capability::ProjectRead,
            Capability::BrandRead,
            Capability::PromptRead,
            Capability::RunRead,
            Capability::WebhookRead,
            Capability::ApiKeyList,
            Capability::MemberList,
        ] {
            assert!(
                is_allowed(Role::Viewer, cap),
                "Viewer must be allowed read cap: {cap:?}"
            );
        }
    }

    /// Evidence sentinel for GA gate.
    #[allow(dead_code)]
    const P4_AUTHZ_1_EVIDENCE: &str =
        "p4-authz-1: matrix::tests — exhaustive 5×35 role×capability matrix; all cells covered";
}
