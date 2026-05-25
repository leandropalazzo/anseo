use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opengeo_analytics::visibility_trend;
use serde::Deserialize;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/visibility/trend", get(trend))
}

#[derive(Debug, Deserialize)]
pub struct TrendQuery {
    pub prompt: String,
    pub days: Option<i32>,
}

async fn trend(
    State(state): State<AppState>,
    Query(q): Query<TrendQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = visibility_trend(
        &state.storage,
        state.project_id,
        &q.prompt,
        q.days.unwrap_or(7),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "visibility trend failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(serde_json::json!({ "points": rows })))
}
