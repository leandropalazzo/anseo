//! `POST /test/seed` — deterministic seeding endpoint for E2E tests
//! (closes the trace-2026-05-27 highest-leverage blocker for Epic 4 P0s:
//! FR-17 / FR-18 / FR-19 / UX-DR40 / NFR-9 / NFR-10).
//!
//! **Test-mode only.** This route is registered into the router *only* when
//! the `ANSEO_TEST_MODE` env var resolves to `1`/`true`. In production the
//! route does not exist and the seed types are not serialized anywhere
//! reachable from `apps/web`.
//!
//! ## Contract
//!
//! Request body (JSON):
//!
//! ```json
//! {
//!   "config_yaml": "<verbatim opengeo.yaml content>",
//!   "runs": [
//!     { "prompt_name": "demo", "provider": "openai",    "status": "ok",
//!       "message_text": "..."                                              },
//!     { "prompt_name": "demo", "provider": "anthropic", "status": "failed",
//!       "error_kind": "provider_rate_limited"                              }
//!   ]
//! }
//! ```
//!
//! Response (200):
//!
//! ```json
//! { "inserted": 2, "run_ids": ["...", "..."] }
//! ```
//!
//! Behavior: the route parses the YAML into a `Config`, constructs one
//! `PromptRunRecord` per `runs[i]`, then calls the same `persist_records`
//! pipeline the orchestrator uses — so seeded data is indistinguishable
//! from a real Prompt Run on disk (same project / prompt upserts, same
//! deterministic IDs, same status / error_kind taxonomy).
//!
//! trace: enables P0-010, P0-011, P0-012, P0-015, P0-016, P0-022, P0-024
//! trace: also unblocks runs-partial-failure.spec.ts AC-6..AC-9

use anseo_core::{Config, ProviderErrorKind, ProviderName, RequestId};
use anseo_providers::orchestrator::{PromptRunRecord, PromptRunStatus};
use anseo_providers::persistence::persist_records;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Build the test-mode router. Callers should only mount this when
/// `ANSEO_TEST_MODE=1`.
pub fn router() -> Router<AppState> {
    Router::new().route("/test/seed", post(seed))
}

#[derive(Debug, Deserialize)]
pub struct SeedRequest {
    /// Verbatim `opengeo.yaml` content (the same shape the CLI accepts).
    /// Determines the project, prompts, and competitor set the seeded runs
    /// belong to.
    pub config_yaml: String,
    /// One entry per Prompt Run row to insert. Order is preserved in the
    /// response's `run_ids` array.
    pub runs: Vec<SeedRun>,
}

#[derive(Debug, Deserialize)]
pub struct SeedRun {
    /// Must match a `prompts[].name` in `config_yaml`.
    pub prompt_name: String,
    pub provider: ProviderName,
    pub status: SeedStatus,
    /// Required when `status == "failed"`.
    #[serde(default)]
    pub error_kind: Option<ProviderErrorKind>,
    /// Required when `status == "ok"` (becomes the LLM response body).
    #[serde(default)]
    pub message_text: Option<String>,
    /// Optional override; defaults to `mock-model`.
    #[serde(default)]
    pub model: Option<String>,
    /// Negative offset from `now()` in seconds. Lets tests spread runs over
    /// a time window (NFR-10 trend chart, etc.).
    #[serde(default)]
    pub started_at_offset_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeedStatus {
    Ok,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct SeedResponse {
    pub inserted: usize,
    pub run_ids: Vec<String>,
}

async fn seed(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<SeedRequest>,
) -> Result<Json<SeedResponse>, (StatusCode, String)> {
    let config = Config::from_yaml_str(&req.config_yaml)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid config_yaml: {e}")))?;
    let project_id = config.project_id();

    // Index prompts by name for O(1) lookup.
    let prompts_by_name: std::collections::HashMap<&str, anseo_core::PromptId> = config
        .prompts
        .iter()
        .filter_map(|p| config.prompt_id(&p.name).map(|id| (p.name.as_str(), id)))
        .collect();

    let mut records = Vec::with_capacity(req.runs.len());
    for run in req.runs {
        let prompt_id = *prompts_by_name
            .get(run.prompt_name.as_str())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    format!(
                        "prompt_name `{}` not present in config_yaml",
                        run.prompt_name
                    ),
                )
            })?;
        let status = match run.status {
            SeedStatus::Ok => PromptRunStatus::Ok,
            SeedStatus::Failed => PromptRunStatus::Failed,
        };
        // Validate the success/failure shape so the dashboard never sees a
        // half-populated row.
        match (status, &run.error_kind, &run.message_text) {
            (PromptRunStatus::Ok, _, None) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "run `{}@{}` has status=ok but no message_text",
                        run.prompt_name,
                        run.provider.as_wire_str()
                    ),
                ));
            }
            (PromptRunStatus::Failed, None, _) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "run `{}@{}` has status=failed but no error_kind",
                        run.prompt_name,
                        run.provider.as_wire_str()
                    ),
                ));
            }
            _ => {}
        }

        let now = Utc::now();
        let started_at =
            now - chrono::Duration::seconds(run.started_at_offset_seconds.unwrap_or(0).max(0));
        let model = run.model.unwrap_or_else(|| "mock-model".to_string());

        records.push(PromptRunRecord {
            id: anseo_core::PromptRunId::new(),
            project_id,
            prompt_id,
            prompt_name: run.prompt_name,
            provider: run.provider,
            provider_model_version: model,
            provider_region: None,
            started_at,
            finished_at: Some(now),
            raw_response: serde_json::json!({"seeded": true}),
            request_parameters: serde_json::json!({}),
            message_text: run.message_text,
            status,
            error_kind: run.error_kind,
            error_message: run.error_kind.map(|k| format!("seeded {k}")),
            request_id: RequestId::new(),
        });
    }

    let persisted = persist_records(&state.storage, &config, &records)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("persist failed: {e}"),
            )
        })?;

    let run_ids = persisted.iter().map(|p| p.run_id.to_string()).collect();
    Ok(Json(SeedResponse {
        inserted: persisted.len(),
        run_ids,
    }))
}

/// Resolve the `ANSEO_TEST_MODE` env var. Returns `true` when the value
/// is `1` / `true` (case-insensitive); `false` otherwise. Centralized here
/// so both the router build path and tests share the same parsing.
pub fn is_enabled_via_env() -> bool {
    matches!(
        std::env::var("ANSEO_TEST_MODE")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("1") | Some("true")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_enabled_accepts_true_and_one() {
        // SAFETY: process env mutation in tests is single-threaded here.
        for (val, expected) in [
            ("1", true),
            ("true", true),
            ("TRUE", true),
            ("True ", true),
            ("0", false),
            ("false", false),
            ("", false),
            ("yes", false),
        ] {
            // SAFETY: see header comment.
            unsafe { std::env::set_var("ANSEO_TEST_MODE", val) };
            assert_eq!(
                is_enabled_via_env(),
                expected,
                "ANSEO_TEST_MODE={val:?} should resolve to {expected}"
            );
        }
        // SAFETY: see header comment.
        unsafe { std::env::remove_var("ANSEO_TEST_MODE") };
        assert!(!is_enabled_via_env());
    }

    #[test]
    fn seed_request_parses_minimal_shape() {
        let raw = r#"{
            "config_yaml": "schema_version: '0.1'\nbrand:\n  name: Acme\nprompts:\n  - name: demo\n    text: hello\nproviders:\n  - name: openai\n    model: gpt-4o-2024-08-06\n",
            "runs": [
                { "prompt_name": "demo", "provider": "openai",    "status": "ok",     "message_text": "hi" },
                { "prompt_name": "demo", "provider": "anthropic", "status": "failed", "error_kind": "provider_rate_limited" }
            ]
        }"#;
        let req: SeedRequest = serde_json::from_str(raw).expect("valid request shape");
        assert_eq!(req.runs.len(), 2);
        assert!(matches!(req.runs[0].status, SeedStatus::Ok));
        assert!(matches!(req.runs[1].status, SeedStatus::Failed));
        assert_eq!(
            req.runs[1].error_kind,
            Some(ProviderErrorKind::ProviderRateLimited)
        );
    }
}
