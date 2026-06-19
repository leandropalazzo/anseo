//! Story 34.2 — Accuracy verdict API surface.
//!
//! `GET  /v1/brands/:brand_id/accuracy`          — list stored accuracy verdicts
//!                                                 for a brand, newest first.
//!                                                 Available on all plans (reads
//!                                                 existing DB rows; no evaluation
//!                                                 is triggered).
//!
//! `POST /v1/brands/:brand_id/accuracy/evaluate` — trigger hallucination
//!                                                 evaluation. In the OSS build
//!                                                 this always returns 402
//!                                                 (`entitlement_required`) because
//!                                                 the `anseo-hallucination` pro
//!                                                 crate is not linked. Pro builds
//!                                                 override this route.
//!
//! All queries use `sqlx::query_as` (no compile-time macros) so the route file
//! can be compiled without a live database or `SQLX_OFFLINE` cache entry.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/brands/:brand_id/accuracy", get(list_verdicts))
        .route("/brands/:brand_id/accuracy/evaluate", post(evaluate_stub))
}

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct VerdictItem {
    pub id: Uuid,
    pub entity: String,
    pub status: String,
    pub rationale: String,
    pub matched_fact: Option<String>,
    pub provider: Option<String>,
    pub severity: String,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct VerdictsResponse {
    pub items: Vec<VerdictItem>,
}

// Row type alias for `sqlx::query_as` — avoids type_complexity clippy lint.
type VerdictRow = (
    Uuid,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    DateTime<Utc>,
);

// ─────────────────────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────────────────────

/// `GET /v1/brands/:brand_id/accuracy`
///
/// Returns all accuracy verdicts stored for the given brand (project), ordered
/// newest-first. The list is available on all plans — it reflects verdicts that
/// were written by a previous Pro evaluation run (or via direct DB insert).
/// An empty list is not an error; it simply means no evaluation has been run
/// yet.
async fn list_verdicts(
    Path(brand_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<VerdictsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let rows: Vec<VerdictRow> = sqlx::query_as(
        r#"
        SELECT id, entity, status, rationale, matched_fact, provider, severity, evaluated_at
          FROM accuracy_verdicts
         WHERE project_id = $1
         ORDER BY evaluated_at DESC
         LIMIT 500
        "#,
    )
    .bind(brand_id)
    .fetch_all(state.storage.pool())
    .await
    .map_err(|e| {
        tracing::warn!(event = "accuracy.list_error", error = %e, brand_id = %brand_id);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "storage_error", "message": e.to_string()})),
        )
    })?;

    let items = rows
        .into_iter()
        .map(
            |(id, entity, status, rationale, matched_fact, provider, severity, evaluated_at)| {
                VerdictItem {
                    id,
                    entity,
                    status,
                    rationale,
                    matched_fact,
                    provider,
                    severity,
                    evaluated_at,
                }
            },
        )
        .collect();

    Ok(Json(VerdictsResponse { items }))
}

/// `POST /v1/brands/:brand_id/accuracy/evaluate`
///
/// OSS stub — always returns 402 `entitlement_required`. The hallucination
/// evaluator is a Pro-only feature (`crates-pro/hallucination`). Pro builds
/// replace this handler with one that checks the org entitlement and dispatches
/// evaluation via `anseo_hallucination::evaluate_claim`.
async fn evaluate_stub(
    Path(brand_id): Path<Uuid>,
    State(_state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    tracing::debug!(
        event = "accuracy.evaluate_entitlement_check",
        brand_id = %brand_id,
        "OSS build — hallucination evaluation requires a Pro entitlement"
    );
    (
        StatusCode::PAYMENT_REQUIRED,
        Json(serde_json::json!({
            "error": "entitlement_required",
            "feature": "hallucination_evaluation",
            "message": "Hallucination evaluation requires an Anseo Pro plan. \
                        Upgrade at https://anseo.io/pricing or contact support."
        })),
    )
}
