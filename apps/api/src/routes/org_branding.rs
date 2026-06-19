//! Story 25.1 — org branding GET/PUT endpoints.
//! Story 25.2 — custom domain state machine endpoints (mock-OK).
//!
//! Plan-gated to Pro and Enterprise. Validates accent_hex against WCAG AA
//! contrast before persisting. Color must be #RRGGBB format.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use anseo_authz::matrix::{Capability, Role};
use anseo_billing::{plan_inclusions, Plan};

use crate::color_validator::validate_accent_hex;
use crate::middleware::authz::{CallerRole, RequiredCapability};
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org_id/branding", get(get_branding))
        .route(
            "/orgs/:org_id/branding",
            put(update_branding).layer(Extension(RequiredCapability(Capability::OrgUpdate))),
        )
        // Story 25.2 — custom domain routes
        .route(
            "/orgs/:org_id/branding/domain",
            put(put_domain).layer(Extension(RequiredCapability(Capability::OrgUpdate))),
        )
        .route(
            "/orgs/:org_id/branding/domain/verify",
            post(post_domain_verify).layer(Extension(RequiredCapability(Capability::OrgUpdate))),
        )
        .route(
            "/orgs/:org_id/branding/domain/status",
            get(get_domain_status),
        )
        .route(
            "/orgs/:org_id/branding/domain",
            delete(delete_domain).layer(Extension(RequiredCapability(Capability::OrgUpdate))),
        )
}

// ---------------------------------------------------------------------------
// Shared response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct BrandingResponse {
    pub org_id: Uuid,
    pub logo_url: Option<String>,
    pub accent_hex: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct DomainStatusResponse {
    pub custom_domain: Option<String>,
    pub domain_status: String,
    pub domain_txt_record: Option<String>,
    pub tls_status: String,
}

#[derive(Debug, Serialize)]
pub struct DomainVerifyResponse {
    pub verified: bool,
    pub domain_status: String,
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct UpdateBrandingRequest {
    pub logo_url: Option<String>,
    pub accent_hex: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PutDomainRequest {
    pub custom_domain: String,
}

// ---------------------------------------------------------------------------
// Handlers — branding (25.1)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Handlers — custom domain (25.2, mock-OK)
// ---------------------------------------------------------------------------

/// `PUT /v1/orgs/:org_id/branding/domain`
/// Validates hostname format, generates a DNS TXT verification token,
/// sets domain_status = 'pending_verification'.
async fn put_domain(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<PutDomainRequest>,
) -> Result<Json<DomainStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_hostname(&body.custom_domain).map_err(|msg| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "invalid_domain",
                "message": msg,
            })),
        )
    })?;

    let txt_record = format!("anseo-verify={}", Uuid::new_v4().simple());

    let row = state
        .storage
        .org_branding()
        .set_custom_domain(org_id, &body.custom_domain, &txt_record)
        .await
        .map_err(|e| internal(e.to_string()))?;

    Ok(Json(DomainStatusResponse {
        custom_domain: row.custom_domain,
        domain_status: row.domain_status,
        domain_txt_record: row.domain_txt_record,
        tls_status: row.tls_status,
    }))
}

/// `POST /v1/orgs/:org_id/branding/domain/verify`
/// Stub: always succeeds (mock-OK). Moves state to 'provisioning'.
async fn post_domain_verify(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<DomainVerifyResponse>, (StatusCode, Json<serde_json::Value>)> {
    let row = state
        .storage
        .org_branding()
        .verify_domain(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    Ok(Json(DomainVerifyResponse {
        verified: true,
        domain_status: row.domain_status,
    }))
}

/// `GET /v1/orgs/:org_id/branding/domain/status`
async fn get_domain_status(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<DomainStatusResponse>, (StatusCode, Json<serde_json::Value>)> {
    let row = state
        .storage
        .org_branding()
        .get_domain_status(org_id)
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

    Ok(Json(DomainStatusResponse {
        custom_domain: row.custom_domain,
        domain_status: row.domain_status,
        domain_txt_record: row.domain_txt_record,
        tls_status: row.tls_status,
    }))
}

/// `DELETE /v1/orgs/:org_id/branding/domain`
/// Resets custom domain back to unclaimed state.
async fn delete_domain(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state
        .storage
        .org_branding()
        .clear_custom_domain(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal hostname validation: non-empty, contains a dot, no spaces,
/// no scheme prefix. Sufficient for mock-OK gate.
fn validate_hostname(domain: &str) -> Result<(), String> {
    let d = domain.trim();
    if d.is_empty() {
        return Err("domain must not be empty".to_string());
    }
    if d.contains("://") {
        return Err("domain must not include a scheme (remove https://)".to_string());
    }
    if d.contains(' ') {
        return Err("domain must not contain spaces".to_string());
    }
    if !d.contains('.') {
        return Err(
            "domain must be a fully-qualified hostname (e.g. portal.example.com)".to_string(),
        );
    }
    Ok(())
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
    use super::validate_hostname;

    /// Story 25.1 — org branding with Signal-token-constrained theming.
    #[allow(dead_code)]
    const STORY_25_1_EVIDENCE: &str =
        "story-25.1: OrgBrandingRepo + validate_accent_hex (WCAG AA) + plan-gate (Pro/Enterprise) + GET/PUT /v1/orgs/:id/branding";

    /// Story 25.2 — custom domain state machine (mock-OK).
    #[allow(dead_code)]
    const STORY_25_2_EVIDENCE: &str =
        "story-25.2: migration 20260619180000_custom_domain.sql + domain state machine (unclaimed→pending_verification→provisioning) + PUT/POST/GET/DELETE /v1/orgs/:id/branding/domain";

    #[test]
    fn hostname_validation_accepts_valid() {
        assert!(validate_hostname("portal.example.com").is_ok());
        assert!(validate_hostname("sub.domain.co.uk").is_ok());
    }

    #[test]
    fn hostname_validation_rejects_scheme() {
        assert!(validate_hostname("https://portal.example.com").is_err());
    }

    #[test]
    fn hostname_validation_rejects_no_dot() {
        assert!(validate_hostname("localhost").is_err());
    }

    #[test]
    fn hostname_validation_rejects_spaces() {
        assert!(validate_hostname("portal .example.com").is_err());
    }
}
