//! Phase 2 Stories 14.2 / 14.3 / 14.4 — analytics HTTP surface.
//!
//! Three GET endpoints that fetch shaped data from Postgres and feed it
//! through the pure compute functions in `anseo_analytics`:
//!
//! - `GET /v1/analytics/citation-graph?days=N` → [`citation_graph::CitationGraph`]
//! - `GET /v1/analytics/heatmap?brand=E&days=N` → [`heatmap::Heatmap`]
//! - `GET /v1/analytics/volatility?prompt=X&provider=Y&brand=E&window=N` → [`volatility::Volatility`]
//!
//! Each query clamps its window to a sane range (matches the underlying
//! `*_rows` / `*_samples` fetchers in `crates/analytics/src/lib.rs`). The
//! routes are gated by `require_api_key` via the `/v1` route layer in
//! `apps/api/src/lib.rs`.

use anseo_analytics::{
    citation_graph, citation_graph_rows, heatmap, heatmap_rows, volatility, volatility_samples,
};
use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

fn err_body(
    status: StatusCode,
    error: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "error": error,
            "message": message,
        })),
    )
}

fn validate_days(value: Option<i32>) -> Result<i32, (StatusCode, Json<serde_json::Value>)> {
    let v = value.unwrap_or(30);
    if !(1..=365).contains(&v) {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_window",
            "`days` must be in [1, 365]",
        ));
    }
    Ok(v)
}

fn validate_window(value: Option<u32>) -> Result<u32, (StatusCode, Json<serde_json::Value>)> {
    let v = value.unwrap_or(volatility::DEFAULT_WINDOW_SAMPLES);
    if !(1..=365).contains(&v) {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_window",
            "`window` must be in [1, 365]",
        ));
    }
    Ok(v)
}

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/analytics/citation-graph", get(citation_graph_handler))
        .route("/analytics/heatmap", get(heatmap_handler))
        .route("/analytics/volatility", get(volatility_handler))
}

#[derive(Debug, Deserialize)]
pub struct WindowQuery {
    pub days: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct HeatmapQuery {
    pub brand: String,
    pub days: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct VolatilityQuery {
    pub prompt: String,
    pub provider: String,
    pub brand: String,
    pub window: Option<u32>,
}

async fn citation_graph_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<citation_graph::CitationGraph>, (StatusCode, Json<serde_json::Value>)> {
    let days = validate_days(q.days)?;
    let rows = citation_graph_rows(&state.storage, project_id, days)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "citation-graph", "fetch failed");
            err_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "citation-graph fetch failed",
            )
        })?;
    Ok(Json(citation_graph::compute(&rows)))
}

async fn heatmap_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<HeatmapQuery>,
) -> Result<Json<heatmap::Heatmap>, (StatusCode, Json<serde_json::Value>)> {
    let brand = q.brand.trim();
    if brand.is_empty() || brand.len() > 256 {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_brand",
            "`brand` must be a non-empty string ≤256 bytes",
        ));
    }
    let days = validate_days(q.days)?;
    let samples = heatmap_rows(&state.storage, project_id, brand, days)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "heatmap", "fetch failed");
            err_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "heatmap fetch failed",
            )
        })?;
    Ok(Json(heatmap::compute(&samples)))
}

async fn volatility_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<VolatilityQuery>,
) -> Result<Json<volatility::Volatility>, (StatusCode, Json<serde_json::Value>)> {
    let prompt = q.prompt.trim();
    let provider = q.provider.trim();
    let brand = q.brand.trim();
    if prompt.is_empty() || provider.is_empty() || brand.is_empty() {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_selector",
            "`prompt`, `provider`, and `brand` are all required and must be non-empty",
        ));
    }
    if prompt.len() > 256 || provider.len() > 64 || brand.len() > 256 {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "selector_too_long",
            "one of `prompt`/`provider`/`brand` exceeds its length cap",
        ));
    }
    let window = validate_window(q.window)?;
    let samples = volatility_samples(&state.storage, project_id, prompt, provider, brand, window)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "volatility", "fetch failed");
            err_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "volatility fetch failed",
            )
        })?;
    Ok(Json(volatility::compute(&samples)))
}
