//! Story 40.5 — `GET /v1/suite/prompts`.
//!
//! Exposes the versioned canonical GEO prompt suite as the minimal operator-
//! facing hook other surfaces can consume without duplicating slug metadata.
//! The source of truth stays in `anseo-benchmark` (Story 39.3); this route is
//! just a read-only projection of `{ slug, version, description }`.

use anseo_benchmark::canonical_geo_prompt_suite;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/suite/prompts", get(list_suite_prompts))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuitePromptSummary {
    pub slug: String,
    pub version: String,
    pub description: String,
}

async fn list_suite_prompts() -> Json<Vec<SuitePromptSummary>> {
    let prompts = canonical_geo_prompt_suite()
        .entries
        .iter()
        .map(|entry| SuitePromptSummary {
            slug: entry.slug.clone(),
            version: entry.version.clone(),
            description: entry.description.clone(),
        })
        .collect();
    Json(prompts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn suite_projection_is_non_empty_and_minimal() {
        let Json(prompts) = list_suite_prompts().await;
        assert!(!prompts.is_empty());
        assert_eq!(prompts[0].slug, "geo-v1/best-vector-db");
        assert_eq!(prompts[0].version, "geo-v1");
        assert!(!prompts[0].description.is_empty());
    }
}
