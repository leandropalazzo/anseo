//! `GET /v1/providers/openrouter/models` — the OpenRouter model catalog.
//!
//! Proxies OpenRouter's public `GET https://openrouter.ai/api/v1/models`
//! (no key required for listing) so the create-schedule form can offer a
//! live dropdown of upstream `<vendor>/<model>` ids to pin. Returns a slim
//! `{ models: [{ id, name }] }` sorted by id.

use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// OpenRouter vendor prefixes that map to OpenGEO's first-class providers — the
/// only ones an operator can also configure standalone with a direct API key.
/// The catalog has 300+ models; narrowing to these keeps the create-schedule
/// dropdown to the vendors that matter for cross-provider consistency.
const FIRST_CLASS_VENDORS: &[&str] = &[
    "openai/",
    "anthropic/",
    "google/",
    "perplexity/",
    "x-ai/",
    "mistralai/",
];

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/providers/openrouter/models", get(list_openrouter_models))
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenRouterModelsResponse {
    pub models: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct UpstreamModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpstreamResponse {
    data: Vec<UpstreamModel>,
}

async fn list_openrouter_models(
) -> Result<Json<OpenRouterModelsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let upstream_err = |msg: &str| {
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": "openrouter_catalog_unavailable",
                "message": msg,
            })),
        )
    };

    let resp = reqwest::Client::new()
        .get(OPENROUTER_MODELS_URL)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "OpenRouter models fetch failed");
            upstream_err("could not reach the OpenRouter model catalog")
        })?;

    if !resp.status().is_success() {
        let code = resp.status();
        tracing::warn!(status = %code, "OpenRouter models returned non-2xx");
        return Err(upstream_err(&format!(
            "OpenRouter model catalog returned HTTP {code}"
        )));
    }

    let parsed: UpstreamResponse = resp.json().await.map_err(|e| {
        tracing::warn!(error = %e, "OpenRouter models response did not parse");
        upstream_err("OpenRouter model catalog returned an unexpected shape")
    })?;

    let mut models: Vec<OpenRouterModel> = parsed
        .data
        .into_iter()
        .filter(|m| FIRST_CLASS_VENDORS.iter().any(|v| m.id.starts_with(v)))
        .map(|m| OpenRouterModel {
            name: m.name.unwrap_or_else(|| m.id.clone()),
            id: m.id,
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Json(OpenRouterModelsResponse { models }))
}
