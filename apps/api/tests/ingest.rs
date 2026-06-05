//! Story 40.1 — `POST /v1/ingest/run` HTTP round-trip (live Postgres).
//!
//! These exercise the full HTTP path: the `X-OpenGEO-Project` guard resolving
//! the scope, the prompt-declared check, and the prompt_run persist. The
//! gate-critical consent/KEK decision logic (opted+KEK → sealed,
//! opted+no-KEK → flagged, stale terms → rejected) is covered exhaustively by
//! the always-run unit tests in `routes::ingest` (`decide_contribution`), which
//! need neither a DB nor a keyring; this file pins the surface around them.
//!
//! Gated on a live DB and `#[ignore]`'d for the default run. Invoke with:
//!
//! ```text
//! DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo \
//!   cargo test -p opengeo-api --test ingest -- --ignored
//! ```

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use opengeo_api::extractors::project::PROJECT_HEADER;
use opengeo_api::{router, AppState};
use opengeo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use opengeo_core::ProjectId;
use opengeo_storage::models::{ProjectRow, PromptRow};
use opengeo_storage::repositories::{
    api_keys::ApiKeyRepo, projects::ProjectRepo, prompts::PromptRepo,
};
use tower::ServiceExt;

/// Seed a uniquely-named project + a declared prompt + an api key, and return
/// the wired router plus the resolution handles. The project NAME is what the
/// `X-OpenGEO-Project` header resolves against, so it must be unique per run.
async fn seeded() -> (axum::Router, ProjectId, String, String) {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for the ingest live-DB tests");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect to test postgres");
    let storage = Arc::new(opengeo_storage::Storage::from_pool(pool.clone()));

    // Derive the project id from its name so the header resolver
    // (`project_id_for_name`) lands on the row we seed.
    let project_name = format!("ingest-fixture-{}", ProjectId::new());
    let project_id = opengeo_core::project_id_for_name(&project_name);
    let now = chrono::Utc::now();
    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: project_name.clone(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");

    PromptRepo::new(&pool)
        .insert(&PromptRow {
            id: opengeo_core::PromptId::new(),
            project_id,
            name: "vector-db".to_string(),
            text: "What is the best vector DB?".to_string(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed prompt");

    let key = gen_key();
    ApiKeyRepo::new(&pool)
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
        configured_project: Arc::new(project_name.clone()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
    };
    (router(state), project_id, project_name, key.plaintext)
}

fn post(uri: &str, key: &str, project_name: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(API_KEY_HEADER, key)
        .header(PROJECT_HEADER, project_name)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_scopes_to_resolved_project_and_persists() {
    let (app, project_id, project_name, api_key) = seeded().await;

    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "response_text": "Pinecone, see https://docs.pinecone.io/guide",
        "observed_rank": 2,
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Scoped to the resolved project.
    assert_eq!(payload["project_id"], project_id.to_string());
    assert_eq!(payload["prompt_slug"], "vector-db");
    // No consent on record ⇒ explicit skip, never a silent contribution.
    assert_eq!(payload["contribution"]["status"], "skipped_not_opted_in");

    // The run was persisted as a prompt_run for the project.
    let run_id: opengeo_core::PromptRunId = payload["run_id"].as_str().unwrap().parse().unwrap();
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let row = opengeo_storage::repositories::prompt_runs::PromptRunRepo::new(&pool)
        .get(run_id)
        .await
        .expect("get run")
        .expect("run persisted");
    assert_eq!(row.provider, "openai");
    assert_eq!(row.provider_model_version, "gpt-4o-2024-08-06");
    assert_eq!(row.status, "ok");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_undeclared_prompt_returns_404() {
    let (app, _project_id, project_name, api_key) = seeded().await;
    let body = serde_json::json!({
        "prompt_slug": "not-declared",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"], "prompt_not_found");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_unknown_project_returns_404() {
    let (app, _project_id, _project_name, api_key) = seeded().await;
    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
    });
    // A project name that resolves to no seeded row ⇒ the header guard 404s
    // before the handler runs.
    let response = app
        .oneshot(post(
            "/v1/ingest/run",
            &api_key,
            "no-such-project-xyz",
            body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"], "project_not_found");
}
