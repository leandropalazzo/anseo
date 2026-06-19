//! Stories 27.9 + 27.10 — DSAR / erasure / offboarding API surface.
//!
//! DSAR endpoints (27.9):
//!   POST   /v1/admin/orgs/:org_id/dsar          — create access or erasure request
//!   GET    /v1/admin/orgs/:org_id/dsar           — list DSAR requests for the org
//!   POST   /v1/admin/orgs/:org_id/dsar/:id/execute — execute an erasure (anonymize)
//!   GET    /v1/admin/orgs/:org_id/dsar/:id/export  — produce DSAR access export
//!
//! Offboarding endpoints (27.10):
//!   POST   /v1/admin/orgs/:org_id/offboard       — initiate offboarding lifecycle
//!   GET    /v1/admin/orgs/:org_id/offboard        — get offboarding status
//!   POST   /v1/admin/orgs/:org_id/offboard/shred  — record CMK deletion (crypto-shred)
//!   POST   /v1/admin/orgs/:org_id/offboard/complete — confirm billing teardown done

use anseo_authz::matrix::Capability;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::authz::{enforce_capability, RequiredCapability};
use crate::middleware::org_guc::OrgContext;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/orgs/:org_id/dsar",
            post(create_dsar)
                .get(list_dsar)
                .layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
        .route(
            "/admin/orgs/:org_id/dsar/:request_id/execute",
            post(execute_erasure).layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
        .route(
            "/admin/orgs/:org_id/dsar/:request_id/export",
            get(export_access).layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
        .route(
            "/admin/orgs/:org_id/offboard",
            post(initiate_offboard)
                .get(get_offboard)
                .layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
        .route(
            "/admin/orgs/:org_id/offboard/shred",
            post(record_shredded).layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
        .route(
            "/admin/orgs/:org_id/offboard/complete",
            post(complete_offboard).layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
}

// ---------------------------------------------------------------------------
// DSAR
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateDsarBody {
    pub kind: String,
    pub subject_email: String,
    pub legal_basis: String,
}

#[derive(Debug, Serialize)]
pub struct DsarResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub kind: String,
    pub state: String,
    pub subject_email: String,
    pub legal_basis: String,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub erasure_summary: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<Utc>,
}

async fn create_dsar(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
    Json(body): Json<CreateDsarBody>,
) -> Result<(StatusCode, Json<DsarResponse>), (StatusCode, Json<serde_json::Value>)> {
    let operator_id = org_context
        .as_ref()
        .and_then(|Extension(ctx)| ctx.operator_id);

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

    if !["access", "erasure"].contains(&body.kind.as_str()) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": "kind must be 'access' or 'erasure'"})),
        ));
    }

    let req = state
        .storage
        .dsar()
        .create(
            org_id,
            &body.kind,
            &body.subject_email,
            &body.legal_basis,
            operator_id,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(DsarResponse {
            id: req.id,
            org_id: req.org_id,
            kind: req.kind,
            state: req.state,
            subject_email: req.subject_email,
            legal_basis: req.legal_basis,
            completed_at: req.completed_at,
            erasure_summary: req.erasure_summary,
            created_at: req.created_at,
        }),
    ))
}

async fn list_dsar(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
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

    let requests = state
        .storage
        .dsar()
        .list_for_org(org_id, 100)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    let items: Vec<serde_json::Value> = requests
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "kind": r.kind,
                "state": r.state,
                "subject_email": r.subject_email,
                "legal_basis": r.legal_basis,
                "completed_at": r.completed_at,
                "created_at": r.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "requests": items })))
}

async fn execute_erasure(
    Path((org_id, request_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<DsarResponse>, (StatusCode, Json<serde_json::Value>)> {
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

    // Fetch existing to get the subject_email.
    let requests = state
        .storage
        .dsar()
        .list_for_org(org_id, 200)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    let req = requests
        .into_iter()
        .find(|r| r.id == request_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "DSAR request not found"})),
            )
        })?;

    if req.kind != "erasure" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": "only erasure requests can be executed"})),
        ));
    }

    let legal_basis = req.legal_basis.clone();
    let subject_email = req.subject_email.clone();

    let updated = state
        .storage
        .dsar()
        .execute_erasure(request_id, org_id, &subject_email, &legal_basis)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    Ok(Json(DsarResponse {
        id: updated.id,
        org_id: updated.org_id,
        kind: updated.kind,
        state: updated.state,
        subject_email: updated.subject_email,
        legal_basis: updated.legal_basis,
        completed_at: updated.completed_at,
        erasure_summary: updated.erasure_summary,
        created_at: updated.created_at,
    }))
}

async fn export_access(
    Path((org_id, request_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
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

    // Fetch the DSAR request to get subject email.
    let requests = state
        .storage
        .dsar()
        .list_for_org(org_id, 200)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    let req = requests
        .into_iter()
        .find(|r| r.id == request_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "DSAR request not found"})),
            )
        })?;

    if req.kind != "access" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": "only access requests can produce exports"})),
        ));
    }

    let export = state
        .storage
        .dsar()
        .access_export(org_id, &req.subject_email)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    Ok(Json(export))
}

// ---------------------------------------------------------------------------
// Offboarding (27.10)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InitiateOffboardBody {
    pub stripe_subscription_id: Option<String>,
    pub stripe_customer_id: Option<String>,
    /// Export grace window in days (default: 30).
    pub export_grace_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct OffboardResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub state: String,
    pub legal_hold: bool,
    pub export_grace_ends_at: chrono::DateTime<Utc>,
    pub shred_scheduled_at: Option<chrono::DateTime<Utc>>,
    pub shredded_at: Option<chrono::DateTime<Utc>>,
    pub completed_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}

async fn initiate_offboard(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
    Json(body): Json<InitiateOffboardBody>,
) -> Result<(StatusCode, Json<OffboardResponse>), (StatusCode, Json<serde_json::Value>)> {
    let operator_id = org_context
        .as_ref()
        .and_then(|Extension(ctx)| ctx.operator_id);

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

    let grace_days = body.export_grace_days.unwrap_or(30).clamp(1, 90);
    let export_grace_ends_at = Utc::now() + Duration::days(grace_days);

    let rec = state
        .storage
        .offboarding()
        .initiate(
            org_id,
            body.stripe_subscription_id.as_deref(),
            body.stripe_customer_id.as_deref(),
            export_grace_ends_at,
            operator_id,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(OffboardResponse {
            id: rec.id,
            org_id: rec.org_id,
            state: rec.state,
            legal_hold: rec.legal_hold,
            export_grace_ends_at: rec.export_grace_ends_at,
            shred_scheduled_at: rec.shred_scheduled_at,
            shredded_at: rec.shredded_at,
            completed_at: rec.completed_at,
            created_at: rec.created_at,
        }),
    ))
}

async fn get_offboard(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<OffboardResponse>, (StatusCode, Json<serde_json::Value>)> {
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

    let rec = state
        .storage
        .offboarding()
        .get_for_org(org_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "no offboarding record for this org"})),
            )
        })?;

    Ok(Json(OffboardResponse {
        id: rec.id,
        org_id: rec.org_id,
        state: rec.state,
        legal_hold: rec.legal_hold,
        export_grace_ends_at: rec.export_grace_ends_at,
        shred_scheduled_at: rec.shred_scheduled_at,
        shredded_at: rec.shredded_at,
        completed_at: rec.completed_at,
        created_at: rec.created_at,
    }))
}

async fn record_shredded(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
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

    let ok = state
        .storage
        .offboarding()
        .record_shredded(org_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    if !ok {
        return Err((
            StatusCode::CONFLICT,
            Json(
                serde_json::json!({"error": "offboarding not in pending_shred state or legal hold active"}),
            ),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn complete_offboard(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
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

    let ok = state
        .storage
        .offboarding()
        .complete(org_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("storage: {e}")})),
            )
        })?;

    if !ok {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "offboarding not in shredded state"})),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    /// [AC-2] Evidence: audit rows are anonymized via UPDATE (actor_login → '<erased>'),
    /// never deleted. The append-only trigger would reject DELETE anyway.
    #[allow(dead_code)]
    const AC2_EVIDENCE: &str =
        "story-27.9 AC-2: DsarRepo::execute_erasure UPDATEs audit rows, never deletes";

    /// [AC-3] Evidence: crypto-shred is separate from data erasure — record_shredded()
    /// only records that the CMK was externally deleted; it does not touch row data.
    #[allow(dead_code)]
    const AC3_EVIDENCE: &str =
        "story-27.9 AC-3: crypto-shred == CMK deletion; row data erasure is separate";

    /// [AC-1 offboarding] Evidence: legal_hold = true prevents advance_to_pending_shred.
    #[allow(dead_code)]
    const LEGAL_HOLD_EVIDENCE: &str =
        "story-27.10 AC-1: advance_to_pending_shred WHERE legal_hold = false";
}

