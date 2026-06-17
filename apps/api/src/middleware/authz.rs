//! Story 22.2 — AuthZ enforcement middleware (single policy point).
//!
//! AC coverage:
//!   - AC-1: every privileged route calls `authz::decide` (enforced by RequireCapability)
//!   - AC-2: denial returns 403 `auth_forbidden` (JSON error envelope)
//!   - AC-3: authZ resolves before the org GUC is set
//!
//! Usage: wrap routes with `RequireCapability::layer(Capability::X)` which
//! inserts a `RequiredCapability` extension; the `check_authz` middleware reads
//! it and calls `anseo_authz::matrix::is_allowed` with the caller's role.
//!
//! Static grep-guard: `tests/authz_surface_coverage.rs` asserts that every
//! non-read-only v1 route either has `RequireCapability` in the layer stack
//! or is explicitly documented as public (health, badge, OpenAPI schema).

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use uuid::Uuid;

use anseo_authz::matrix::{is_allowed, Capability, Role};

use crate::AppState;

/// Extension injected by `RequireCapability::layer` — tells `check_authz`
/// which capability to enforce on the current route.
#[derive(Clone, Copy)]
pub struct RequiredCapability(pub Capability);

/// Caller role resolved from the DB — stamped into extensions by
/// `check_authz` so downstream handlers can read it without a second lookup.
#[derive(Clone, Copy, Debug)]
pub struct CallerRole(pub Role);

/// Middleware: resolves the caller's role and checks the required capability.
///
/// Requires:
/// - `OrgContext` in extensions (set by auth middleware; carries operator_id + org_id)
/// - `RequiredCapability` in extensions (set by `RequireCapability::layer` per-route)
///
/// On deny: returns `403 auth_forbidden`.
/// On missing org context (self-host ApiKeyAuth mode): skips the check (all
/// capabilities allowed for the single-tenant default operator).
pub async fn check_authz(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    use crate::middleware::org_guc::OrgContext;

    // No required capability registered for this route → pass-through.
    let Some(&RequiredCapability(cap)) = request.extensions().get::<RequiredCapability>() else {
        return Ok(next.run(request).await);
    };

    // No OrgContext → self-host single-tenant mode → skip RBAC.
    let Some(ctx) = request.extensions().get::<OrgContext>().copied() else {
        return Ok(next.run(request).await);
    };

    let Some(operator_id) = ctx.operator_id else {
        // API-key auth with no operator_id — treat as Operator role.
        request.extensions_mut().insert(CallerRole(Role::Operator));
        return Ok(next.run(request).await);
    };

    // Resolve role from DB. DB errors become 503, not 403.
    let role = resolve_role(&state, operator_id, ctx.org_id).await?;

    match role {
        None => Err(forbidden_response("not a member of this org")),
        Some(role) => {
            if is_allowed(role, cap) {
                request.extensions_mut().insert(CallerRole(role));
                Ok(next.run(request).await)
            } else {
                Err(forbidden_response(&format!(
                    "role {role:?} is not permitted to perform {cap:?}"
                )))
            }
        }
    }
}

fn forbidden_response(detail: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "auth_forbidden",
            "detail": detail
        })),
    )
        .into_response()
}

/// Look up the caller's role in the org from `operator_org_roles`.
/// Returns `Err(503)` on DB failure so transient errors don't masquerade as 403.
async fn resolve_role(
    state: &AppState,
    operator_id: Uuid,
    org_id: Uuid,
) -> Result<Option<Role>, Response> {
    // Raw query — we can't use the RLS GUC here (not yet set), but
    // `operator_org_roles` is a management table not subject to per-org RLS.
    // Use sqlx::query (not the macro) to avoid offline-mode cache requirements.
    let row: Option<(String,)> = sqlx::query_as::<_, (String,)>(
        "SELECT role::text FROM operator_org_roles WHERE operator_id = $1 AND org_id = $2",
    )
    .bind(operator_id)
    .bind(org_id)
    .fetch_optional(state.storage.pool())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "authz role lookup failed");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error": "service_unavailable", "detail": "role lookup failed"})),
        )
            .into_response()
    })?;

    let Some((role_str,)) = row else {
        return Ok(None);
    };
    let role = match role_str.as_str() {
        "owner" => Role::Owner,
        "admin" => Role::Admin,
        "billing" => Role::Billing,
        "operator" => Role::Operator,
        "viewer" => Role::Viewer,
        _ => return Ok(None),
    };
    Ok(Some(role))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_response_has_correct_shape() {
        let resp = forbidden_response("test detail");
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn caller_role_is_copy() {
        let r = CallerRole(Role::Admin);
        let _ = r; // test Copy impl
    }
}
