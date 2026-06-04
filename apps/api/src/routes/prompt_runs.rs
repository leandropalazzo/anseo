//! Phase 2 Story 12.2 — REST write surface (minimal slice).
//!
//! `POST /v1/prompt-runs` accepts a JSON body declaring a one-shot
//! Prompt Run and dispatches it through the orchestrator. Returns 202
//! Accepted with the new run's id. This is the minimal write endpoint
//! that unblocks downstream Story 12.3 (SDKs) and the k6 burst load
//! scenario in `tests/load/k6/scenarios/webhook_burst.js` — both need
//! a way to *originate* prompt runs without going through the CLI.
//!
//! Deferred to a follow-up round:
//! - Cursor pagination on list endpoints (P2-120).
//! - YAML round-trip writeback for prompt + schedule declarations
//!   (P0-125 — needs a comment-preserving YAML AST library).
//! - Validation against the OpenAPI artifact (P1-124 — depends on
//!   the OpenAPI generator deferred from Story 12.1).
//!
//! Auth: gated by `require_api_key` via the `/v1` route layer in
//! `apps/api/src/lib.rs`. The `AuthenticatedProject` extractor enforces
//! cross-project scoping the same way the SSE route does.

use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/prompt-runs", post(create_prompt_run))
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePromptRunRequest {
    pub prompt_name: String,
    pub provider: String,
    #[serde(default)]
    pub triggered_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatePromptRunResponse {
    pub status: String,
    pub run_id: String,
    pub project_id: String,
    pub prompt_name: String,
    pub provider: String,
    pub dispatched_at: chrono::DateTime<chrono::Utc>,
}

/// Validate the request shape. Returns the canonical (prompt_name,
/// provider) tuple ready for orchestrator dispatch, or a structured
/// 400 message. Pure-logic — the orchestrator dispatch lives behind
/// the `Storage` accessor in [`create_prompt_run`] (deferred to a
/// follow-up round once the orchestrator's API-mode entry point lands).
pub fn validate_request(request: &CreatePromptRunRequest) -> Result<(), String> {
    let trimmed = request.prompt_name.trim();
    if trimmed.is_empty() {
        return Err("`prompt_name` must not be empty".to_string());
    }
    if trimmed != request.prompt_name {
        return Err(format!(
            "`prompt_name` `{}` has leading or trailing whitespace; trim it first",
            request.prompt_name
        ));
    }
    if !is_slug_safe(trimmed) {
        return Err(format!(
            "`prompt_name` `{}` is not slug-safe (lowercase ASCII + digits + hyphens)",
            request.prompt_name
        ));
    }
    if !is_supported_provider(&request.provider) {
        return Err(format!(
            "unknown provider `{}` (supported: openai, anthropic, gemini, perplexity, grok, mistral, openrouter, mock)",
            request.provider
        ));
    }
    Ok(())
}

fn is_slug_safe(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_supported_provider(s: &str) -> bool {
    matches!(
        s,
        "openai"
            | "anthropic"
            | "gemini"
            | "perplexity"
            | "grok"
            | "mistral"
            | "openrouter"
            | "mock"
    )
}

async fn create_prompt_run(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Json(request): Json<CreatePromptRunRequest>,
) -> Result<(StatusCode, Json<CreatePromptRunResponse>), (StatusCode, Json<serde_json::Value>)> {
    if let Err(msg) = validate_request(&request) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "validation_failed",
                "message": msg,
            })),
        ));
    }

    // Look up the prompt by name within the authenticated project. The
    // API requires the operator to have already declared the prompt via
    // `ogeo prompt add` or YAML — undeclared names get a 404 with a
    // pointer rather than auto-creating a row.
    let prompt = opengeo_storage::repositories::prompts::PromptRepo::new(state.storage.pool())
        .find_by_name(project_id, &request.prompt_name)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "prompt lookup failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "failed to look up prompt",
                })),
            )
        })?;
    let Some(prompt) = prompt else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "prompt_not_found",
                "message": format!(
                    "prompt `{}` is not declared in this project — add it with `ogeo prompt add` first",
                    request.prompt_name
                ),
            })),
        ));
    };

    // Mock provider: synchronous run-and-persist. Deterministic body so
    // downstream tooling (SDK acceptance + k6 burst scenario) get a
    // real persisted run round-trip. Live providers still come through
    // the CLI orchestrator path — wiring the orchestrator's API-mode
    // entry point lands in a follow-up.
    if request.provider == "mock" {
        let run_id = opengeo_core::PromptRunId::new();
        let now = chrono::Utc::now();
        let mock_response = serde_json::json!({
            "kind": "mock",
            "prompt_name": request.prompt_name,
            "note": "Deterministic mock response from POST /v1/prompt-runs",
        });
        let row = opengeo_storage::models::PromptRunRow {
            id: run_id,
            prompt_id: prompt.id,
            provider: "mock".to_string(),
            provider_model_version: "mock-1.0".to_string(),
            provider_region: None,
            started_at: now,
            finished_at: Some(now),
            raw_response: mock_response,
            request_parameters: serde_json::json!({
                "triggered_by": request.triggered_by,
            }),
            status: "ok".to_string(),
            error_kind: None,
            organization_id: None,
            tenant_id: None,
            created_at: now,
        };
        opengeo_storage::repositories::prompt_runs::PromptRunRepo::new(state.storage.pool())
            .insert(&row)
            .await
            .map_err(|e| {
                // FK violation (`23503`) means the prompt was deleted
                // between find_by_name and insert; report that as 404
                // rather than a generic 500.
                if let opengeo_storage::Error::Sqlx(sqlx::Error::Database(db_err)) = &e {
                    if db_err.code().as_deref() == Some("23503") {
                        tracing::warn!(error = %e, "prompt deleted between lookup and insert");
                        return (
                            StatusCode::NOT_FOUND,
                            Json(serde_json::json!({
                                "error": "prompt_not_found",
                                "message": "prompt was deleted between lookup and insert; retry will re-validate",
                            })),
                        );
                    }
                }
                tracing::error!(error = %e, "prompt run insert failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({
                        "error": "persist_failed",
                        "message": "failed to persist mock prompt run",
                    })),
                )
            })?;
        return Ok((
            StatusCode::ACCEPTED,
            Json(CreatePromptRunResponse {
                status: "ok".to_string(),
                run_id: run_id.to_string(),
                project_id: project_id.to_string(),
                prompt_name: request.prompt_name,
                provider: request.provider,
                dispatched_at: now,
            }),
        ));
    }

    // Live providers: drive through the boot-loaded `Config` +
    // `ProviderRegistry` via the orchestrator's `run_all` entry point,
    // scoped to the requested (prompt_name, provider) tuple. Missing-
    // key providers are absent from the registry and the orchestrator
    // synthesises a `failed` record for them — that record is still
    // persisted, so the caller gets a 202 and can read the failure
    // reason via `GET /v1/runs/:id`.
    let Some(config) = state.config.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "orchestrator_unconfigured",
                "message": "API server booted without a readable `opengeo.yaml` (set OGEO_CONFIG or place opengeo.yaml in CWD); live providers cannot be dispatched.",
            })),
        ));
    };
    let Some(registry) = state.provider_registry.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "orchestrator_unconfigured",
                "message": "API server booted without a provider registry; check logs for the boot-time failure.",
            })),
        ));
    };

    let Some(provider_name) = opengeo_core::ProviderName::parse(&request.provider) else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "validation_failed",
                "message": format!("unknown provider `{}`", request.provider),
            })),
        ));
    };

    let filter = opengeo_providers::OrchestratorFilter {
        prompt_names: Some(vec![request.prompt_name.clone()]),
        providers: Some(vec![provider_name]),
    };
    let orchestrator =
        opengeo_providers::Orchestrator::new((**config).clone(), (**registry).clone());
    let records = orchestrator.run_all(filter).await;

    // The filter narrows to exactly the (prompt_name, provider) cell;
    // the orchestrator emits one record either way (success, provider
    // error, or `unregistered_record` for missing-key providers).
    let Some(record) = records.into_iter().next() else {
        // Should be unreachable — the request's prompt_name was just
        // validated against the same Config the orchestrator iterates,
        // and the provider is in the registry (or synthesised). Surface
        // as 500 rather than silently dropping.
        tracing::error!(
            prompt = %request.prompt_name,
            provider = %request.provider,
            "orchestrator returned no record for narrowed filter"
        );
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "internal_error",
                "message": "orchestrator returned no record for the requested cell",
            })),
        ));
    };

    let now = chrono::Utc::now();
    let run_id = record.id;
    let message_text = record.message_text.clone();
    let raw_response_for_extraction = record.raw_response.clone();
    let row = opengeo_storage::models::PromptRunRow {
        id: record.id,
        prompt_id: prompt.id,
        provider: record.provider.as_wire_str().to_string(),
        provider_model_version: record.provider_model_version.clone(),
        provider_region: record.provider_region.clone(),
        started_at: record.started_at,
        finished_at: record.finished_at,
        raw_response: record.raw_response.clone(),
        request_parameters: {
            // Merge the orchestrator's request_parameters with the
            // request's `triggered_by` annotation so the persisted row
            // carries provenance the same way the mock path does.
            let mut params = record.request_parameters.clone();
            if let Some(obj) = params.as_object_mut() {
                obj.insert(
                    "triggered_by".to_string(),
                    serde_json::json!(request.triggered_by),
                );
            }
            params
        },
        status: record.status.as_wire_str().to_string(),
        error_kind: record.error_kind.map(|k| k.as_wire_str().to_string()),
        organization_id: None,
        tenant_id: None,
        created_at: now,
    };
    opengeo_storage::repositories::prompt_runs::PromptRunRepo::new(state.storage.pool())
        .insert(&row)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "prompt run insert failed (live provider path)");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "persist_failed",
                    "message": "failed to persist live prompt run",
                })),
            )
        })?;

    // Parse the response into mentions + citations so the analytics surfaces
    // (brand rank, visibility, share of voice) have data. `config` carries the
    // DB brand overlay (brand name + competitors). Best-effort: extraction
    // failure is logged but does not fail the already-persisted run.
    if let Some(text) = message_text.as_deref() {
        if let Err(e) = opengeo_extractors::extract_and_persist(
            &state.storage,
            config,
            run_id,
            text,
            &raw_response_for_extraction,
            now,
        )
        .await
        {
            tracing::warn!(error = %e, "mention/citation extraction failed for live run");
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(CreatePromptRunResponse {
            status: record.status.as_wire_str().to_string(),
            run_id: record.id.to_string(),
            project_id: project_id.to_string(),
            prompt_name: request.prompt_name,
            provider: request.provider,
            dispatched_at: record.started_at,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_canonical_request() {
        let req = CreatePromptRunRequest {
            prompt_name: "vector-db".into(),
            provider: "openai".into(),
            triggered_by: None,
        };
        assert!(validate_request(&req).is_ok());
    }

    #[test]
    fn validate_rejects_empty_prompt_name() {
        let req = CreatePromptRunRequest {
            prompt_name: "  ".into(),
            provider: "openai".into(),
            triggered_by: None,
        };
        let err = validate_request(&req).unwrap_err();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn validate_rejects_non_slug_prompt_name() {
        let req = CreatePromptRunRequest {
            prompt_name: "Vector DB".into(),
            provider: "openai".into(),
            triggered_by: None,
        };
        let err = validate_request(&req).unwrap_err();
        assert!(err.contains("slug-safe"));
    }

    #[test]
    fn validate_rejects_unknown_provider() {
        let req = CreatePromptRunRequest {
            prompt_name: "vector-db".into(),
            provider: "bogus".into(),
            triggered_by: None,
        };
        let err = validate_request(&req).unwrap_err();
        assert!(err.contains("unknown provider"));
        assert!(err.contains("openai"));
    }

    #[test]
    fn validate_accepts_all_phase2_providers() {
        for p in [
            "openai",
            "anthropic",
            "gemini",
            "perplexity",
            "grok",
            "mistral",
            "openrouter",
            "mock",
        ] {
            let req = CreatePromptRunRequest {
                prompt_name: "vector-db".into(),
                provider: p.into(),
                triggered_by: None,
            };
            assert!(
                validate_request(&req).is_ok(),
                "provider `{p}` should validate"
            );
        }
    }

    #[test]
    fn create_prompt_run_request_deserializes_from_canonical_json() {
        let raw = r#"{"prompt_name":"vec","provider":"openai","triggered_by":"api"}"#;
        let req: CreatePromptRunRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(req.prompt_name, "vec");
        assert_eq!(req.provider, "openai");
        assert_eq!(req.triggered_by.as_deref(), Some("api"));
    }

    #[test]
    fn create_prompt_run_request_optional_triggered_by() {
        let raw = r#"{"prompt_name":"vec","provider":"openai"}"#;
        let req: CreatePromptRunRequest = serde_json::from_str(raw).unwrap();
        assert!(req.triggered_by.is_none());
    }
}
