//! GET /api/runs — paginated list backing the Dashboard runs view (FR-17).
//! GET /api/runs/:id — detail view (FR-18). Phase 1: the row plus best-effort
//! mention/citation summary (empty arrays until extraction runs).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opengeo_analytics::{list_runs, RunListParams};
use opengeo_core::PromptRunId;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/runs", get(list))
        .route("/api/runs/:id", get(detail))
}

/// Phase 2 `/v1` mount — same handlers, paths without the `/api/` prefix.
/// Nested under `/v1` in `apps/api/src/lib.rs`, so the public surface
/// becomes `/v1/runs` and `/v1/runs/:id`.
pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/runs", get(list))
        .route("/runs/:id", get(detail))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    project: crate::extractors::EffectiveProject,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let params = RunListParams {
        limit: q.limit.unwrap_or(25),
        offset: q.offset.unwrap_or(0),
    };
    let rows = list_runs(&state.storage, project.id(), params)
        .await
        .map_err(internal)?;
    Ok(Json(serde_json::json!({ "runs": rows })))
}

#[derive(Debug, Serialize)]
struct RunDetail {
    id: String,
    prompt_id: String,
    provider: String,
    provider_model_version: String,
    started_at: chrono::DateTime<chrono::Utc>,
    finished_at: Option<chrono::DateTime<chrono::Utc>>,
    status: String,
    error_kind: Option<String>,
    raw_response: serde_json::Value,
    request_parameters: serde_json::Value,
}

async fn detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<RunDetail>, StatusCode> {
    let run_id = PromptRunId::from_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;
    let row = state
        .storage
        .prompt_runs()
        .get(run_id)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(RunDetail {
        id: row.id.to_string(),
        prompt_id: row.prompt_id.to_string(),
        provider: row.provider,
        provider_model_version: row.provider_model_version,
        started_at: row.started_at,
        finished_at: row.finished_at,
        status: row.status,
        error_kind: row.error_kind,
        raw_response: row.raw_response,
        request_parameters: row.request_parameters,
    }))
}

fn internal<E: std::fmt::Display>(e: E) -> StatusCode {
    tracing::error!(error = %e, "internal API error");
    StatusCode::INTERNAL_SERVER_ERROR
}
