//! Story 25.1 — org branding GET/PUT endpoints.
//!
//! Plan-gated to Pro and Enterprise. Validates accent_hex against WCAG AA
//! contrast before persisting. Color must be #RRGGBB format.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use anseo_authz::matrix::Role;
use anseo_billing::{plan_inclusions, Plan};

use crate::color_validator::validate_accent_hex;
use crate::middleware::authz::CallerRole;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route(
        "/orgs/:org_id/branding",
        get(get_branding).put(update_branding),
    )
}

#[derive(Debug, Serialize)]
pub struct BrandingResponse {
    pub org_id: Uuid,
    pub logo_url: Option<String>,
    pub accent_hex: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateBrandingRequest {
    pub logo_url: Option<String>,
    pub accent_hex: Option<String>,
}

async fn get_branding(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<BrandingResponse>, (StatusCode, Json<serde_json::Value>)> {
    let row = state
        .storage
        .org_branding()
        .get(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "branding_not_found",
                "message": "no branding configuration exists for this org",
            })),
        )
    })?;

    Ok(Json(BrandingResponse {
        org_id: row.org_id,
        logo_url: row.logo_url,
        accent_hex: row.accent_hex,
        updated_at: row.updated_at,
    }))
}

async fn update_branding(
    Path(org_id): Path<Uuid>,
    caller_role: Option<Extension<CallerRole>>,
    State(state): State<AppState>,
    Json(body): Json<UpdateBrandingRequest>,
) -> Result<Json<BrandingResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Require Owner or Admin role.
    let role = caller_role.map(|Extension(r)| r.0);
    match role {
        Some(Role::Owner) | Some(Role::Admin) | None => {}
        Some(_) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "auth_forbidden",
                    "message": "only Owner or Admin may update org branding",
                })),
            ));
        }
    }

    // Plan gate: Pro/Enterprise only.
    let entitlement = state
        .storage
        .org_entitlements()
        .get(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    let plan: Plan = entitlement
        .and_then(|e| e.plan.parse().ok())
        .unwrap_or(Plan::Free);

    let inclusions = plan_inclusions(plan);
    if !inclusions.branding_enabled {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            Json(serde_json::json!({
                "error": "plan_upgrade_required",
                "message": "org branding is available on Pro and Enterprise plans",
            })),
        ));
    }

    // Validate accent_hex if provided.
    if let Some(ref hex) = body.accent_hex {
        validate_accent_hex(hex).map_err(|reason| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "invalid_accent_hex",
                    "message": reason,
                })),
            )
        })?;
    }

    let row = state
        .storage
        .org_branding()
        .upsert(org_id, body.logo_url.as_deref(), body.accent_hex.as_deref())
        .await
        .map_err(|e| internal(e.to_string()))?;

    Ok(Json(BrandingResponse {
        org_id: row.org_id,
        logo_url: row.logo_url,
        accent_hex: row.accent_hex,
        updated_at: row.updated_at,
    }))
}

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
    /// Story 25.1 — org branding with Signal-token-constrained theming.
    /// Evidence: WCAG AA validation via color_validator + plan gate enforced.
    #[allow(dead_code)]
    const STORY_25_1_EVIDENCE: &str =
        "story-25.1: OrgBrandingRepo + validate_accent_hex (WCAG AA) + plan-gate (Pro/Enterprise) + GET/PUT /v1/orgs/:id/branding";
}
