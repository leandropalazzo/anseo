//! Story 0.10 — `GET /v1/prompts/similarity-index`.
//!
//! The browser extension calls this endpoint with the text the user is
//! typing into a chat surface; the response is the set of configured
//! prompts whose Jaccard similarity (estimated via MinHash) clears a
//! caller-supplied threshold. The extension uses it to decide whether to
//! surface inline rank data for the matched prompt(s).
//!
//! Pure-Rust MinHash lives in `anseo_core::similarity`; this handler is
//! a thin adapter that pulls configured prompts from the project's
//! `Config`, builds an index, runs the query, and shapes the response.
//!
//! # Index caching strategy
//!
//! Per-request rebuild. The configured prompt set is small (Phase 3
//! ceiling is in the low hundreds) and building a 128-function MinHash
//! signature over each prompt is microseconds; carrying a cached index
//! in `AppState` would force us to invalidate on every Config reload
//! and adds lifetime plumbing for no measurable win. If a future Phase
//! profiles this as hot we can promote it to `OnceCell<Arc<...>>` on
//! `AppState`; today we rebuild.
//!
//! # Auth
//!
//! Mounted under `/v1`, so `require_api_key` in `apps/api/src/lib.rs`
//! gates it. `X-Anseo-Project` is accepted and ignored per the Phase 3
//! single-project model — documented but not enforced.

use anseo_core::similarity::{sha256_input, MinHashIndex, NUM_HASH_FUNCTIONS};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

const MAX_INPUT_CHARS: usize = 4096;
const DEFAULT_THRESHOLD: f32 = 0.6;
const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 50;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/prompts/similarity-index", get(similarity_index))
}

#[derive(Debug, Deserialize)]
pub struct SimilarityQuery {
    pub text: String,
    #[serde(default)]
    pub threshold: Option<f32>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SimilarityMatch {
    pub name: String,
    pub prompt: String,
    pub estimated_jaccard: f32,
    /// Always `null` in Phase 3 — `anseo.yaml` does not carry per-prompt
    /// configuration timestamps. The field is reserved so the extension
    /// can render a "configured at" badge once we surface it (likely from
    /// the prompts DB row's `created_at` once Story 0.11 lands).
    pub configured_at: Option<String>,
    /// Whether the API can serve rank data for this prompt. Phase 3:
    /// `true` for every match (the prompt is by definition in the project
    /// config and the worker is responsible for keeping data fresh). The
    /// extension still calls the per-prompt endpoint to actually fetch.
    pub rank_data_available: bool,
}

#[derive(Debug, Serialize)]
pub struct SimilarityResponse {
    pub input_hash: String,
    pub matches: Vec<SimilarityMatch>,
    pub method: &'static str,
    pub num_hash_functions: usize,
}

async fn similarity_index(
    State(state): State<AppState>,
    Query(q): Query<SimilarityQuery>,
) -> Result<Json<SimilarityResponse>, StatusCode> {
    if q.text.is_empty() || q.text.chars().count() > MAX_INPUT_CHARS {
        return Err(StatusCode::BAD_REQUEST);
    }
    let threshold = q.threshold.unwrap_or(DEFAULT_THRESHOLD);
    if !(0.0..=1.0).contains(&threshold) || threshold.is_nan() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    if limit == 0 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Project Config carries the canonical prompt declarations. When
    // absent (dev binds without a loaded YAML) we return an empty match
    // set rather than 503 — the extension treats "no matches" as "no
    // overlay" and the absence of YAML is a configuration, not a
    // failure, state.
    let prompts: Vec<(String, String)> = match state.config.as_ref() {
        Some(cfg) => cfg
            .prompts
            .iter()
            .map(|p| (p.name.clone(), p.text.clone()))
            .collect(),
        None => Vec::new(),
    };

    let index = MinHashIndex::build(&prompts);
    let matches = index.query(&q.text, threshold, limit);

    let response = SimilarityResponse {
        input_hash: sha256_input(&q.text),
        matches: matches
            .into_iter()
            .map(|m| SimilarityMatch {
                name: m.name,
                prompt: m.prompt,
                estimated_jaccard: m.estimated_jaccard,
                configured_at: None,
                rank_data_available: true,
            })
            .collect(),
        method: "minhash",
        num_hash_functions: NUM_HASH_FUNCTIONS,
    };
    Ok(Json(response))
}

#[cfg(test)]
mod tests {
    //! Pure handler-shape tests live in
    //! `apps/api/tests/prompts_similarity.rs` since they need the full
    //! router. Unit-test surface here is intentionally small: validation
    //! constants only.

    use super::*;

    #[test]
    fn limit_default_is_under_cap() {
        const { assert!(DEFAULT_LIMIT <= MAX_LIMIT) };
    }

    #[test]
    fn threshold_default_in_unit_interval() {
        assert!((0.0..=1.0).contains(&DEFAULT_THRESHOLD));
    }
}
