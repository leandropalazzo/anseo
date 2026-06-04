//! Story 30-6 — Per-run extraction read API.
//!
//! `GET /api/runs/:id` (see `routes::runs`) returns only the raw run record.
//! These endpoints surface the extraction data already persisted for a run so
//! the dashboard run-detail panels can render real data:
//!
//! - `GET /runs/:id/mentions`    — mentions for the run (entity, provider,
//!   ranking/position, confidence if available).
//! - `GET /runs/:id/citations`   — citations for the run (domain, url,
//!   source_type, provider).
//! - `GET /runs/:id/provenance`  — provenance/lifecycle steps for the run.
//!   There is NO provenance / lifecycle-log table in the Phase 1 schema
//!   (see `crates/storage/migrations/`), so this returns an empty list
//!   rather than inventing schema.
//! - `GET /runs/:id/responses`   — the raw response text per provider for the
//!   run. A `prompt_runs` row is a single (run, provider) pair, so this is a
//!   one-element list keyed by the run's provider.
//!
//! All endpoints are read-only and use the RUNTIME `sqlx::query_as` form via
//! the existing `mentions().list_by_run()` / `citations().list_by_run()`
//! storage helpers, keeping the offline `.sqlx/` cache untouched.
//!
//! Wire shapes are defined locally with `#[derive(Serialize)]` — they do NOT
//! touch the frozen MCP wire types. Mirrors the route style in `runs.rs`
//! (AppState, `internal` error mapping). An unknown run id returns `404`
//! (consistent with `runs::detail`); a known run with no rows returns `[]`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opengeo_core::PromptRunId;
use serde::Serialize;
use std::str::FromStr;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/runs/:id/mentions", get(mentions))
        .route("/api/runs/:id/citations", get(citations))
        .route("/api/runs/:id/provenance", get(provenance))
        .route("/api/runs/:id/responses", get(responses))
}

/// Phase 2 `/v1` mount — same handlers, paths without the `/api/` prefix.
/// Nested under `/v1` in `apps/api/src/lib.rs`.
pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/runs/:id/mentions", get(mentions))
        .route("/runs/:id/citations", get(citations))
        .route("/runs/:id/provenance", get(provenance))
        .route("/runs/:id/responses", get(responses))
}

/// One extracted mention. `provider` is denormalised from the parent run so
/// the dashboard can build its entity×provider matrix without a second fetch.
#[derive(Debug, Serialize)]
struct MentionEntry {
    id: String,
    entity: String,
    provider: String,
    /// Ranking / position of the mention in the response (the `rank` column).
    rank: i32,
    /// Character offset of the match in the raw response text.
    char_offset: i32,
    matched_text: String,
    sentiment_label: Option<String>,
    sentiment_score: Option<i16>,
    sentiment_lane: Option<String>,
}

/// One citation. `provider` is denormalised from the parent run.
#[derive(Debug, Serialize)]
struct CitationEntry {
    id: String,
    domain: String,
    url: Option<String>,
    source_type: Option<String>,
    frequency: i32,
    provider: String,
}

/// A provenance / lifecycle step for the run (Story 31-3). Read from the
/// `run_provenance` table the orchestrator write path populates as a run
/// flows through its stages. `detail` carries the per-step free-form JSON
/// (e.g. provider name, error kind, or extraction counts).
#[derive(Debug, Serialize)]
struct ProvenanceStep {
    step: String,
    status: String,
    at: chrono::DateTime<chrono::Utc>,
    detail: serde_json::Value,
}

/// The raw response captured for one (run, provider) pair.
#[derive(Debug, Serialize)]
struct ResponseEntry {
    provider: String,
    provider_model_version: String,
    status: String,
    raw_response: serde_json::Value,
}

fn parse_run_id(id: &str) -> Result<PromptRunId, StatusCode> {
    PromptRunId::from_str(id).map_err(|_| StatusCode::BAD_REQUEST)
}

/// Resolve the run row, mapping a missing run to `404` so callers can
/// distinguish "no such run" from "run exists but has no rows" (`[]`).
async fn require_run(
    state: &AppState,
    run_id: PromptRunId,
) -> Result<opengeo_storage::models::PromptRunRow, StatusCode> {
    state
        .storage
        .prompt_runs()
        .get(run_id)
        .await
        .map_err(internal)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn mentions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<MentionEntry>>, StatusCode> {
    let run_id = parse_run_id(&id)?;
    let run = require_run(&state, run_id).await?;
    let rows = state
        .storage
        .mentions()
        .list_by_run(run_id)
        .await
        .map_err(internal)?;
    let out = rows
        .into_iter()
        .map(|r| MentionEntry {
            id: r.id.to_string(),
            entity: r.entity,
            provider: run.provider.clone(),
            rank: r.rank,
            char_offset: r.char_offset,
            matched_text: r.matched_text,
            sentiment_label: r.sentiment_label,
            sentiment_score: r.sentiment_score,
            sentiment_lane: r.sentiment_lane,
        })
        .collect();
    Ok(Json(out))
}

async fn citations(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<CitationEntry>>, StatusCode> {
    let run_id = parse_run_id(&id)?;
    let run = require_run(&state, run_id).await?;
    let rows = state
        .storage
        .citations()
        .list_by_run(run_id)
        .await
        .map_err(internal)?;
    let out = rows
        .into_iter()
        .map(|r| CitationEntry {
            id: r.id.to_string(),
            domain: r.domain,
            url: r.url,
            source_type: r.source_type,
            frequency: r.frequency,
            provider: run.provider.clone(),
        })
        .collect();
    Ok(Json(out))
}

async fn provenance(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ProvenanceStep>>, StatusCode> {
    let run_id = parse_run_id(&id)?;
    // 404 if the run does not exist; otherwise the provenance steps recorded
    // by the orchestrator write path, ordered oldest-first by `at`.
    require_run(&state, run_id).await?;
    let rows = state
        .storage
        .run_provenance()
        .list_by_run(run_id)
        .await
        .map_err(internal)?;
    let out = rows
        .into_iter()
        .map(|r| ProvenanceStep {
            step: r.step,
            status: r.status,
            at: r.at,
            detail: r.detail,
        })
        .collect();
    Ok(Json(out))
}

async fn responses(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ResponseEntry>>, StatusCode> {
    let run_id = parse_run_id(&id)?;
    let run = require_run(&state, run_id).await?;
    // A prompt_runs row is a single (run, provider) pair, so the per-provider
    // response list has exactly one element.
    Ok(Json(vec![ResponseEntry {
        provider: run.provider,
        provider_model_version: run.provider_model_version,
        status: run.status,
        raw_response: run.raw_response,
    }]))
}

fn internal<E: std::fmt::Display>(e: E) -> StatusCode {
    tracing::error!(error = %e, "internal API error");
    StatusCode::INTERNAL_SERVER_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use opengeo_core::api_key::{generate as gen_key, API_KEY_HEADER};
    use opengeo_core::{ProjectId, PromptId};
    use opengeo_storage::models::{CitationRow, MentionRow, ProjectRow, PromptRow, PromptRunRow};
    use std::sync::Arc;
    use tower::ServiceExt;

    /// Seeds project + prompt + run (+ optional mention/citation) and returns
    /// an authenticated router, the api key, and the seeded run id.
    async fn seed(pool: sqlx::PgPool) -> (axum::Router, String, PromptRunId) {
        let storage = Arc::new(opengeo_storage::Storage::from_pool(pool.clone()));
        let now = chrono::Utc::now();

        let project_id = ProjectId::new();
        storage
            .projects()
            .insert(&ProjectRow {
                id: project_id,
                name: format!("test-{project_id}"),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await
            .expect("seed project");

        let prompt_id = PromptId::new();
        storage
            .prompts()
            .insert(&PromptRow {
                id: prompt_id,
                project_id,
                name: "fixture-prompt".into(),
                text: "What is the best vector DB?".into(),
                tags: Vec::new(),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await
            .expect("seed prompt");

        let run_id = PromptRunId::new();
        storage
            .prompt_runs()
            .insert(&PromptRunRow {
                id: run_id,
                prompt_id,
                provider: "openai".into(),
                provider_model_version: "gpt-4o-mini".into(),
                provider_region: None,
                started_at: now,
                finished_at: Some(now),
                raw_response: serde_json::json!({"text": "Pinecone is great."}),
                request_parameters: serde_json::json!({}),
                status: "ok".into(),
                error_kind: None,
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await
            .expect("seed run");

        storage
            .mentions()
            .insert(&MentionRow {
                id: opengeo_core::MentionId::new(),
                prompt_run_id: run_id,
                entity: "Pinecone".into(),
                char_offset: 0,
                rank: 1,
                matched_text: "Pinecone".into(),
                sentiment_label: Some("positive".into()),
                sentiment_score: Some(80),
                sentiment_lane: Some("deterministic_lexicon".into()),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await
            .expect("seed mention");

        storage
            .citations()
            .insert(&CitationRow {
                id: opengeo_core::CitationId::new(),
                prompt_run_id: run_id,
                url: Some("https://pinecone.io/docs".into()),
                domain: "pinecone.io".into(),
                frequency: 1,
                source_type: Some("docs".into()),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await
            .expect("seed citation");

        let key = gen_key();
        storage
            .api_keys()
            .insert(
                project_id,
                "fixture-key",
                &key.sha256_hash,
                &key.display_prefix,
            )
            .await
            .expect("seed api key");

        let (events, _rx) = opengeo_scheduler::worker::event_channel();
        let state = AppState {
            storage,
            project_id,
            events,
            config: None,
            provider_registry: None,
            configured_project: Arc::new("default".to_string()),
            setup_install_state: Arc::new(tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
        };
        (crate::router(state), key.plaintext, run_id)
    }

    async fn get_json(
        app: &axum::Router,
        uri: &str,
        api_key: &str,
    ) -> (StatusCode, serde_json::Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(API_KEY_HEADER, api_key)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let json = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
        };
        (status, json)
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn endpoints_return_seeded_rows(pool: sqlx::PgPool) {
        let (app, key, run_id) = seed(pool).await;
        let rid = run_id.to_string();

        let (status, body) = get_json(&app, &format!("/v1/runs/{rid}/mentions"), &key).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["entity"], "Pinecone");
        assert_eq!(body[0]["provider"], "openai");
        assert_eq!(body[0]["rank"], 1);
        assert_eq!(body[0]["sentiment_label"], "positive");
        assert_eq!(body[0]["sentiment_score"], 80);
        assert_eq!(body[0]["sentiment_lane"], "deterministic_lexicon");

        let (status, body) = get_json(&app, &format!("/v1/runs/{rid}/citations"), &key).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["domain"], "pinecone.io");
        assert_eq!(body[0]["url"], "https://pinecone.io/docs");
        assert_eq!(body[0]["source_type"], "docs");
        assert_eq!(body[0]["provider"], "openai");

        let (status, body) = get_json(&app, &format!("/v1/runs/{rid}/responses"), &key).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["provider"], "openai");
        assert_eq!(body[0]["raw_response"]["text"], "Pinecone is great.");

        // Provenance starts empty for a hand-seeded run (the test fixture
        // inserts the run directly, not via the orchestrator write path).
        let (status, body) = get_json(&app, &format!("/v1/runs/{rid}/provenance"), &key).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 0);
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn provenance_returns_steps_ordered_by_at(pool: sqlx::PgPool) {
        let (app, key, run_id) = seed(pool.clone()).await;
        let rid = run_id.to_string();

        // Seed provenance rows out of insertion order via explicit `at`
        // timestamps so we can prove the endpoint orders by `at`.
        let storage = Arc::new(opengeo_storage::Storage::from_pool(pool));
        let base = chrono::Utc::now();
        let rid_uuid = uuid::Uuid::from_bytes(run_id.into_ulid().to_bytes());
        // Insert "ranking" last-in-time first, then earlier steps, to confirm
        // ordering is by `at` and not by row insertion order.
        for (step, status, detail, offset_secs) in [
            (
                "ranking",
                "skipped",
                serde_json::json!({"reason": "pending"}),
                4i64,
            ),
            (
                "provider_call",
                "ok",
                serde_json::json!({"provider": "openai"}),
                0,
            ),
            (
                "response_persisted",
                "ok",
                serde_json::json!({"status": "ok"}),
                1,
            ),
            (
                "mention_extraction",
                "ok",
                serde_json::json!({"count": 2}),
                2,
            ),
            (
                "citation_extraction",
                "ok",
                serde_json::json!({"count": 1}),
                3,
            ),
        ] {
            let at = base + chrono::Duration::seconds(offset_secs);
            sqlx::query(
                "INSERT INTO run_provenance (prompt_run_id, step, status, detail, at) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(rid_uuid)
            .bind(step)
            .bind(status)
            .bind(detail)
            .bind(at)
            .execute(storage.pool())
            .await
            .expect("seed provenance row");
        }

        let (status, body) = get_json(&app, &format!("/v1/runs/{rid}/provenance"), &key).await;
        assert_eq!(status, StatusCode::OK);
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 5);
        // Ordered oldest-first by `at`, independent of insertion order.
        let steps: Vec<&str> = arr.iter().map(|r| r["step"].as_str().unwrap()).collect();
        assert_eq!(
            steps,
            vec![
                "provider_call",
                "response_persisted",
                "mention_extraction",
                "citation_extraction",
                "ranking",
            ]
        );
        assert_eq!(arr[0]["status"], "ok");
        assert_eq!(arr[0]["detail"]["provider"], "openai");
        assert_eq!(arr[4]["status"], "skipped");
        assert_eq!(arr[2]["detail"]["count"], 2);
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn unknown_run_returns_404(pool: sqlx::PgPool) {
        let (app, key, _run_id) = seed(pool).await;
        let unknown = PromptRunId::new().to_string();
        for path in ["mentions", "citations", "responses", "provenance"] {
            let (status, _) = get_json(&app, &format!("/v1/runs/{unknown}/{path}"), &key).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "path {path}");
        }
    }
}
