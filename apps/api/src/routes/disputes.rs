//! Disputes API — Story 43.6 (full disputes lifecycle).
//!
//! Public submission + read surface (no user account required — AC-5) and the
//! operator-review workflow. Domain control (DNS-TXT) is the sole arbiter for
//! claim conflicts and change-of-control. Every state change is audited.
//!
//! Public:
//!   `POST /v1/disputes`                       — submit a dispute (any of the
//!                                               five types). AC-1/3/4/5, BD-3.
//!   `GET  /v1/disputes/:id`                   — fetch one dispute + its status.
//!   `GET  /v1/disputes/:id/events`            — audit trail (NFR5).
//!
//! Operator review:
//!   `GET  /v1/disputes`                       — review queue (open+under_review).
//!   `POST /v1/disputes/:id/approve`           — approve a correction (AC-1).
//!   `POST /v1/disputes/:id/reject`            — reject with grounds (AC-1).
//!   `POST /v1/disputes/:id/adjudicate`        — claim-conflict (AC-2).
//!   `POST /v1/disputes/:id/transfer`          — change-of-control.
//!   `POST /v1/disputes/:id/gdpr-assess`       — GDPR Art.21 (AC-3).
//!
//! BD-3: no endpoint here references billing or a premium tier.
//!
//! Dynamic sqlx only via the repository layer — no `query!` macros.

use anseo_storage::repositories::disputes::{DisputeRecord, DISPUTE_TYPES};
use anseo_storage::repositories::entities::EntityRepo;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        // Public submission + read (AC-5: no user account required).
        .route("/disputes", post(submit_dispute).get(list_pending))
        .route("/disputes/:id", get(get_dispute))
        .route("/disputes/:id/events", get(get_events))
        // Operator review workflow.
        .route("/disputes/:id/approve", post(approve))
        .route("/disputes/:id/reject", post(reject))
        .route("/disputes/:id/adjudicate", post(adjudicate))
        .route("/disputes/:id/transfer", post(transfer))
        .route("/disputes/:id/gdpr-assess", post(gdpr_assess))
}

type ApiError = (StatusCode, Json<serde_json::Value>);

fn storage_err(e: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
    )
}

fn not_found(id: uuid::Uuid) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "dispute_not_found", "id": id })),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Views
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DisputeView {
    pub id: uuid::Uuid,
    pub domain: String,
    pub dispute_type: String,
    pub status: String,
    pub description: String,
    pub submitter_email: Option<String>,
    pub proposed_value: Option<String>,
    pub suppressed: bool,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolution_grounds: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<DisputeRecord> for DisputeView {
    fn from(r: DisputeRecord) -> Self {
        Self {
            id: r.id,
            domain: r.domain,
            dispute_type: r.dispute_type,
            status: r.status,
            description: r.description,
            submitter_email: r.submitter_email,
            proposed_value: r.proposed_value,
            suppressed: r.suppressed,
            resolved_by: r.resolved_by,
            resolved_at: r.resolved_at,
            resolution_grounds: r.resolution_grounds,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/disputes  — public submission (AC-1/3/4/5, BD-3)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    pub domain: String,
    pub dispute_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub submitter_email: Option<String>,
    /// Correction requests: the proposed new display name.
    #[serde(default)]
    pub proposed_value: Option<String>,
}

async fn submit_dispute(
    State(state): State<AppState>,
    Json(body): Json<SubmitRequest>,
) -> Result<(StatusCode, Json<DisputeView>), ApiError> {
    let domain = EntityRepo::normalize_domain(&body.domain);
    if domain.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "empty_domain" })),
        ));
    }
    let dispute_type = body.dispute_type.trim();
    if !DISPUTE_TYPES.contains(&dispute_type) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_dispute_type",
                "allowed": DISPUTE_TYPES,
            })),
        ));
    }

    let record = state
        .storage
        .disputes()
        .submit(
            &domain,
            dispute_type,
            body.description.trim(),
            body.submitter_email.as_deref().map(str::trim),
            body.proposed_value.as_deref().map(str::trim),
        )
        .await
        .map_err(storage_err)?;

    Ok((StatusCode::CREATED, Json(DisputeView::from(record))))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/disputes  — operator review queue
// ─────────────────────────────────────────────────────────────────────────────

async fn list_pending(State(state): State<AppState>) -> Result<Json<Vec<DisputeView>>, ApiError> {
    let rows = state
        .storage
        .disputes()
        .pending()
        .await
        .map_err(storage_err)?;
    Ok(Json(rows.into_iter().map(DisputeView::from).collect()))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/disputes/:id
// ─────────────────────────────────────────────────────────────────────────────

async fn get_dispute(
    Path(id): Path<uuid::Uuid>,
    State(state): State<AppState>,
) -> Result<Json<DisputeView>, ApiError> {
    match state
        .storage
        .disputes()
        .get(id)
        .await
        .map_err(storage_err)?
    {
        Some(r) => Ok(Json(DisputeView::from(r))),
        None => Err(not_found(id)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/disputes/:id/events  — audit trail (NFR5)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EventView {
    pub id: uuid::Uuid,
    pub event_type: String,
    pub actor: String,
    pub rationale: Option<String>,
    pub detail: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn get_events(
    Path(id): Path<uuid::Uuid>,
    State(state): State<AppState>,
) -> Result<Json<Vec<EventView>>, ApiError> {
    let events = state
        .storage
        .disputes()
        .events(id)
        .await
        .map_err(storage_err)?;
    Ok(Json(
        events
            .into_iter()
            .map(|e| EventView {
                id: e.id,
                event_type: e.event_type,
                actor: e.actor,
                rationale: e.rationale,
                detail: e.detail,
                created_at: e.created_at,
            })
            .collect(),
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Operator actions
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    pub operator: String,
    #[serde(default)]
    pub rationale: String,
}

async fn approve(
    Path(id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    Json(body): Json<ApproveRequest>,
) -> Result<Json<DisputeView>, ApiError> {
    let rec = state
        .storage
        .disputes()
        .approve_correction(id, body.operator.trim(), body.rationale.trim())
        .await
        .map_err(|e| map_repo_err(e, id))?;
    Ok(Json(DisputeView::from(rec)))
}

#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub operator: String,
    /// Plain-language reason + appeals-path explanation (AC-1).
    pub grounds: String,
}

async fn reject(
    Path(id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    Json(body): Json<RejectRequest>,
) -> Result<Json<DisputeView>, ApiError> {
    let rec = state
        .storage
        .disputes()
        .reject(id, body.operator.trim(), body.grounds.trim())
        .await
        .map_err(|e| map_repo_err(e, id))?;
    Ok(Json(DisputeView::from(rec)))
}

#[derive(Debug, Deserialize)]
pub struct AdjudicateRequest {
    pub operator: String,
    /// Email of the party that produced the DNS-TXT proof (the winner).
    pub winner_email: String,
    /// Email of the losing party to notify (optional).
    #[serde(default)]
    pub loser_email: Option<String>,
    #[serde(default)]
    pub rationale: String,
}

async fn adjudicate(
    Path(id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    Json(body): Json<AdjudicateRequest>,
) -> Result<Json<DisputeView>, ApiError> {
    let rec = state
        .storage
        .disputes()
        .adjudicate_claim_conflict(
            id,
            body.operator.trim(),
            body.winner_email.trim(),
            body.loser_email.as_deref().map(str::trim),
            body.rationale.trim(),
        )
        .await
        .map_err(|e| map_repo_err(e, id))?;
    Ok(Json(DisputeView::from(rec)))
}

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub operator: String,
    pub new_owner_email: String,
    #[serde(default)]
    pub rationale: String,
}

async fn transfer(
    Path(id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    Json(body): Json<TransferRequest>,
) -> Result<Json<DisputeView>, ApiError> {
    let rec = state
        .storage
        .disputes()
        .transfer_control(
            id,
            body.operator.trim(),
            body.new_owner_email.trim(),
            body.rationale.trim(),
        )
        .await
        .map_err(|e| map_repo_err(e, id))?;
    Ok(Json(DisputeView::from(rec)))
}

#[derive(Debug, Deserialize)]
pub struct GdprAssessRequest {
    pub operator: String,
    /// True if the objection is honored (genuinely personal data, no compelling
    /// legitimate grounds override) — processing then stops (AC-3).
    pub honored: bool,
    /// Documented grounds for the decision (stored in the audit log).
    pub grounds: String,
}

async fn gdpr_assess(
    Path(id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    Json(body): Json<GdprAssessRequest>,
) -> Result<Json<DisputeView>, ApiError> {
    let rec = state
        .storage
        .disputes()
        .assess_gdpr_objection(id, body.operator.trim(), body.honored, body.grounds.trim())
        .await
        .map_err(|e| map_repo_err(e, id))?;
    Ok(Json(DisputeView::from(rec)))
}

fn map_repo_err(e: anseo_storage::error::Error, id: uuid::Uuid) -> ApiError {
    match e {
        anseo_storage::error::Error::NotFound => not_found(id),
        other => storage_err(other),
    }
}
