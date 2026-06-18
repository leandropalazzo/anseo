//! Story 20.8 — `/v1/orgs` substrate endpoints (D-P4-10).
//!
//! Provides the operator-plane org management surface. These endpoints are
//! authenticated via the existing API-key middleware. RBAC enforcement (owner/
//! admin role required for write operations) lands in Story 22.1.
//!
//! Routes:
//!   GET    /v1/orgs              — list all organizations
//!   POST   /v1/orgs              — create a new organization
//!   GET    /v1/orgs/:org_id      — get org details
//!   GET    /v1/orgs/:org_id/brands — list brands (projects) for an org

use anseo_storage::repositories::organizations::{OrgBrandRow, OrgRow};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedOperator;
use crate::middleware::authz::CallerRole;
use crate::middleware::org_guc::OrgContext;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/orgs", get(list_orgs).post(create_org))
        .route("/orgs/:org_id", get(get_org))
        .route("/orgs/:org_id/brands", get(list_org_brands))
}

// ── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OrgResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub region: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<OrgRow> for OrgResponse {
    fn from(r: OrgRow) -> Self {
        Self {
            id: r.id,
            slug: r.slug,
            name: r.name,
            region: r.region,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OrgsListResponse {
    pub items: Vec<OrgResponse>,
}

#[derive(Debug, Serialize)]
pub struct BrandResponse {
    pub id: Uuid,
    pub brand_id: Uuid,
    pub name: String,
    pub site_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<OrgBrandRow> for BrandResponse {
    fn from(r: OrgBrandRow) -> Self {
        Self {
            id: r.id,
            brand_id: r.brand_id,
            name: r.name,
            site_url: r.site_url,
            created_at: r.created_at,
            archived_at: r.archived_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BrandsListResponse {
    pub items: Vec<BrandResponse>,
}

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub slug: String,
    pub name: String,
    pub region: Option<String>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// GET /v1/orgs — list all organizations.
async fn list_orgs(
    State(state): State<AppState>,
) -> Result<Json<OrgsListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let orgs = state
        .storage
        .orgs()
        .list()
        .await
        .map_err(|e| internal(e.to_string()))?;

    Ok(Json(OrgsListResponse {
        items: orgs.into_iter().map(OrgResponse::from).collect(),
    }))
}

/// POST /v1/orgs — create a new organization.
async fn create_org(
    op: Option<axum::extract::Extension<AuthenticatedOperator>>,
    State(state): State<AppState>,
    Json(body): Json<CreateOrgRequest>,
) -> Result<(StatusCode, Json<OrgResponse>), (StatusCode, Json<serde_json::Value>)> {
    let org = state
        .storage
        .orgs()
        .create(&body.slug, &body.name, body.region.as_deref())
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("unique") || msg.contains("duplicate") {
                (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({
                        "error": "slug_conflict",
                        "message": "an organization with this slug already exists",
                    })),
                )
            } else {
                internal(msg)
            }
        })?;

    // Story 26.1 — emit audit event. Fire-and-forget: a storage failure must
    // not fail the org creation itself.
    let actor = op
        .and_then(|axum::extract::Extension(o)| o.actor)
        .unwrap_or_else(|| "system".into());
    let meta = serde_json::json!({ "slug": org.slug, "name": org.name });
    if let Err(e) = state
        .storage
        .org_audit()
        .append(org.id, None, &actor, "org.create", None, Some(&meta))
        .await
    {
        tracing::warn!(error = %e, org_id = %org.id, "org audit append failed");
    }

    Ok((StatusCode::CREATED, Json(OrgResponse::from(org))))
}

/// GET /v1/orgs/:org_id — get org details.
async fn get_org(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<OrgResponse>, (StatusCode, Json<serde_json::Value>)> {
    let org = state
        .storage
        .orgs()
        .get(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "org_not_found",
                    "message": "organization not found",
                })),
            )
        })?;

    Ok(Json(OrgResponse::from(org)))
}

/// GET /v1/orgs/:org_id/brands — list brands (projects) for an org.
async fn list_org_brands(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    caller_role: Option<Extension<CallerRole>>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<BrandsListResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Verify org exists first.
    state
        .storage
        .orgs()
        .get(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "org_not_found",
                    "message": "organization not found",
                })),
            )
        })?;

    let role = caller_role.map(|Extension(role)| role.0);
    let operator_id = org_context.and_then(|Extension(ctx)| ctx.operator_id);
    let brands = if let (Some(role), Some(operator_id)) = (role, operator_id) {
        if anseo_authz::role_bypasses_brand_grants(role) {
            state
                .storage
                .orgs()
                .list_brands(org_id)
                .await
                .map_err(|e| internal(e.to_string()))?
        } else if anseo_authz::role_requires_brand_grant(role) {
            state
                .storage
                .orgs()
                .list_brands_granted_to(org_id, operator_id)
                .await
                .map_err(|e| internal(e.to_string()))?
        } else {
            Vec::new()
        }
    } else {
        // Self-host API-key mode has no Phase 4 operator context.
        state
            .storage
            .orgs()
            .list_brands(org_id)
            .await
            .map_err(|e| internal(e.to_string()))?
    };

    Ok(Json(BrandsListResponse {
        items: brands.into_iter().map(BrandResponse::from).collect(),
    }))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn internal(msg: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": "internal_error",
            "message": msg,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn org_response_serializes() {
        let resp = OrgResponse {
            id: uuid::Uuid::nil(),
            slug: "test".into(),
            name: "Test".into(),
            region: None,
            created_at: chrono::Utc::now(),
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"slug\":\"test\""));
    }

    #[test]
    fn create_org_request_deserializes() {
        let json = r#"{"slug":"my-org","name":"My Org"}"#;
        let req: CreateOrgRequest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(req.slug, "my-org");
        assert_eq!(req.name, "My Org");
        assert!(req.region.is_none());
    }
}
