use anseo_analytics::sentiment::sentiment_points;
use anseo_analytics::{visibility_matrix, visibility_trend, visibility_trend_all};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/visibility/trend", get(trend))
        .route("/api/visibility/overall", get(overall))
        .route("/api/visibility/sentiment", get(sentiment))
}

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/visibility/trend", get(trend))
        .route("/visibility/overall", get(overall))
        .route("/visibility/sentiment", get(sentiment))
}

#[derive(Debug, Deserialize)]
pub struct TrendQuery {
    pub prompt: String,
    pub days: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OverallQuery {
    pub days: Option<i32>,
}

/// Resolve the project's primary brand entity for mention matching: the DB
/// brand row wins, falling back to the bootstrap config so a fresh project
/// still renders. Empty when neither is present.
async fn primary_brand_entity(state: &AppState, project_id: anseo_core::ProjectId) -> String {
    if let Ok(Some(row)) = state.storage.projects().get_brand(project_id).await {
        return row.name;
    }
    state
        .config
        .as_ref()
        .map(|c| c.brand.name.clone())
        .unwrap_or_default()
}

async fn trend(
    State(state): State<AppState>,
    project: crate::extractors::EffectiveProject,
    Query(q): Query<TrendQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows = visibility_trend(&state.storage, project.id(), &q.prompt, q.days.unwrap_or(7))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "visibility trend failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::json!({ "points": rows })))
}

async fn overall(
    State(state): State<AppState>,
    project: crate::extractors::EffectiveProject,
    Query(q): Query<OverallQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let days = q.days.unwrap_or(30);
    let brand = primary_brand_entity(&state, project.id()).await;

    let matrix = visibility_matrix(&state.storage, project.id(), &brand, days)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "visibility matrix failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let trend = visibility_trend_all(&state.storage, project.id(), &brand, days)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "visibility overall trend failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "brand": brand,
        "window_days": days,
        "matrix": matrix,
        "trend": trend,
    })))
}

async fn sentiment(
    State(state): State<AppState>,
    project: crate::extractors::EffectiveProject,
    Query(q): Query<OverallQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let days = q.days.unwrap_or(30).clamp(1, 365);
    let points = sentiment_points(&state.storage, project.id(), days)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "visibility sentiment failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(serde_json::json!({
        "window_days": days,
        "points": points,
    })))
}
