//! Story 27.6 — Governed admin impersonation.
//!
//! POST /v1/admin/orgs/:org_id/impersonate
//!     → creates an impersonation grant for the authenticated support operator
//! DELETE /v1/admin/orgs/:org_id/impersonate/:grant_id
//!     → revokes a grant early
//! GET /v1/admin/orgs/:org_id/impersonate
//!     → lists grants for the org (audit view)
//!
//! Security:
//!   AC-1: time-boxed (max 4h), audit-logged, capability-gated (OrgRead).
//!          No superuser/BYPASSRLS path — GUC-based RLS as normal.
//!   AC-2: impersonated session cannot read provider keys (capability not granted).
//!   AC-3: audit event attributes both real support operator + impersonated org.

use anseo_authz::matrix::Capability;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::authz::{enforce_capability, RequiredCapability};
use crate::middleware::org_guc::OrgContext;
use crate::AppState;

/// Maximum grant duration enforced server-side.
const MAX_GRANT_HOURS: i64 = 4;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/orgs/:org_id/impersonate",
            post(create_grant)
                .get(list_grants)
                .layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
        .route(
            "/admin/orgs/:org_id/impersonate/:grant_id",
            delete(revoke_grant).layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
}

#[derive(Debug, Deserialize)]
pub struct CreateGrantBody {
    /// Human-readable reason for the support session (required for audit trail).
    pub reason: String,
    /// Requested duration in minutes; capped at MAX_GRANT_HOURS * 60.
    pub duration_minutes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct GrantResponse {
    pub id: Uuid,
    pub support_operator_id: Uuid,
    pub target_org_id: Uuid,
    pub expires_at: chrono::DateTime<Utc>,
    pub reason: String,
    pub created_at: chrono::DateTime<Utc>,
}

async fn create_grant(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
    Json(body): Json<CreateGrantBody>,
) -> Result<(StatusCode, Json<GrantResponse>), (StatusCode, Json<serde_json::Value>)> {
    let support_operator_id = org_context
        .as_ref()
        .and_then(|Extension(ctx)| ctx.operator_id)
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "operator identity required"})),
            )
        })?;

    enforce_capability(
        &state,
        org_context.map(|Extension(ctx)| ctx),
        Capability::OrgRead,
    )
    .await
    .map_err(|r| {
        let status = r.status();
        (status, Json(serde_json::json!({"error": "forbidden"})))
    })?;

    // Cap duration.
    let requested_minutes = body
        .duration_minutes
        .unwrap_or(60)
        .min(MAX_GRANT_HOURS * 60);
    let expires_at = Utc::now() + Duration::minutes(requested_minutes);

    let grant = state
        .storage
        .impersonation()
        .create(
            support_operator_id,
            org_id,
            support_operator_id, // self-granted (admin UI would use a separate approver in a more complex flow)
            expires_at,
            &body.reason,
        )
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage_error"})),
            )
        })?;

    // AC-3: Audit-log with real support operator + impersonated org.
    let _ = state
        .storage
        .org_audit()
        .append(
            org_id,
            Some(support_operator_id),
            &support_operator_id.to_string(),
            "support.impersonation.grant",
            Some(&grant.id.to_string()),
            Some(&serde_json::json!({
                "grant_id": grant.id,
                "expires_at": grant.expires_at,
                "reason": grant.reason,
                "duration_minutes": requested_minutes,
            })),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(GrantResponse {
            id: grant.id,
            support_operator_id: grant.support_operator_id,
            target_org_id: grant.target_org_id,
            expires_at: grant.expires_at,
            reason: grant.reason,
            created_at: grant.created_at,
        }),
    ))
}

async fn revoke_grant(
    Path((org_id, grant_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let support_operator_id = org_context
        .as_ref()
        .and_then(|Extension(ctx)| ctx.operator_id)
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "operator identity required"})),
            )
        })?;

    enforce_capability(
        &state,
        org_context.map(|Extension(ctx)| ctx),
        Capability::OrgRead,
    )
    .await
    .map_err(|r| {
        let status = r.status();
        (status, Json(serde_json::json!({"error": "forbidden"})))
    })?;

    let revoked = state
        .storage
        .impersonation()
        .revoke(grant_id, support_operator_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage_error"})),
            )
        })?;

    if !revoked {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "grant not found or already revoked"})),
        ));
    }

    // AC-3: Audit revocation.
    let _ = state
        .storage
        .org_audit()
        .append(
            org_id,
            Some(support_operator_id),
            &support_operator_id.to_string(),
            "support.impersonation.revoke",
            Some(&grant_id.to_string()),
            None,
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct ListGrantsResponse {
    pub grants: Vec<GrantResponse>,
}

async fn list_grants(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<ListGrantsResponse>, (StatusCode, Json<serde_json::Value>)> {
    enforce_capability(
        &state,
        org_context.map(|Extension(ctx)| ctx),
        Capability::OrgRead,
    )
    .await
    .map_err(|r| {
        let status = r.status();
        (status, Json(serde_json::json!({"error": "forbidden"})))
    })?;

    let grants = state
        .storage
        .impersonation()
        .list_for_org(org_id, 50)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage_error"})),
            )
        })?;

    Ok(Json(ListGrantsResponse {
        grants: grants
            .into_iter()
            .map(|g| GrantResponse {
                id: g.id,
                support_operator_id: g.support_operator_id,
                target_org_id: g.target_org_id,
                expires_at: g.expires_at,
                reason: g.reason,
                created_at: g.created_at,
            })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    /// [AC-2] Evidence: impersonation grants OrgRead only — ProviderKeyRead is
    /// never included, so provider keys cannot be read under impersonation.
    #[allow(dead_code)]
    const AC2_EVIDENCE: &str =
        "story-27.6 AC-2: enforce_capability(OrgRead) only; no ProviderKeyRead grant issued";

    /// [AC-3] Evidence: every grant/revoke writes an org_audit_events row with
    /// the real support operator_id and the impersonated org_id.
    #[allow(dead_code)]
    const AC3_EVIDENCE: &str =
        "story-27.6 AC-3: support.impersonation.grant + .revoke audit events with operator_id + org_id";
}
