use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opengeo_analytics::citation_summary;
use serde::Deserialize;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/citations/summary", get(summary))
}

#[derive(Debug, Deserialize)]
pub struct SummaryQuery {
    pub limit: Option<i64>,
}

async fn summary(
    State(state): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = citation_summary(&state.storage, state.project_id, q.limit.unwrap_or(50))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "citation summary failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::json!({ "domains": rows })))
}
