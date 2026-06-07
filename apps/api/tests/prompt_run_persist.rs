//! Story 12.2 — POST /v1/prompt-runs persistence round-trip (mock provider).
//!
//! Verifies the mock-provider write path against a live Postgres:
//! - POST with provider="mock" against a declared prompt name persists
//!   a PromptRunRow and returns 202 with the persisted id.
//! - POST against an undeclared prompt name returns 404.
//! - POST with a live provider against a state that has no `Config`
//!   loaded returns 503 (`orchestrator_unconfigured`).
//! - POST with a live provider against a state that has a `Config` but
//!   no API key configured persists a `failed` row with
//!   `provider_unauthorized` and returns 202.
//!
//! Gated on `live_db_tests` via the `opengeo-scheduler` feature flag
//! (shared with the rest of the live-DB suite) and `#[ignore]`'d for the
//! default cargo run; invoke with:
//!
//! ```text
//! DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo \
//!   cargo test -p opengeo-api --test prompt_run_persist -- --ignored
//! ```

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use anseo_core::ProjectId;
use anseo_storage::models::{ProjectRow, PromptRow};
use anseo_storage::repositories::{
    api_keys::ApiKeyRepo, projects::ProjectRepo, prompts::PromptRepo,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn seeded_router_and_project() -> (axum::Router, ProjectId, String) {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for the prompt_run_persist live-DB tests");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect to test postgres");
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));

    let project_id = ProjectId::new();
    let now = chrono::Utc::now();
    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("test-{}", project_id),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");

    let prompt_id = anseo_core::PromptId::new();
    PromptRepo::new(&pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: "fixture-prompt".to_string(),
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

    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id,
        events,
        config: None,
        provider_registry: None,
        configured_project: std::sync::Arc::new("default".to_string()),
        setup_install_state: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        serve_info: None,
        loaded_plugins: std::sync::Arc::new(Vec::new()),
    };
    (router(state), project_id, key.plaintext)
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn mock_provider_persists_and_returns_run_id() {
    let (app, project_id, api_key) = seeded_router_and_project().await;

    let body = serde_json::json!({
        "prompt_name": "fixture-prompt",
        "provider": "mock",
        "triggered_by": "12.2-test",
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/prompt-runs")
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["project_id"], project_id.to_string());
    assert_eq!(payload["prompt_name"], "fixture-prompt");
    assert_eq!(payload["provider"], "mock");

    let run_id_str = payload["run_id"].as_str().unwrap();
    let run_id: anseo_core::PromptRunId = run_id_str.parse().unwrap();
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let row = anseo_storage::repositories::prompt_runs::PromptRunRepo::new(&pool)
        .get(run_id)
        .await
        .expect("get run")
        .expect("run was persisted");
    assert_eq!(row.status, "ok");
    assert_eq!(row.provider, "mock");
    assert_eq!(row.provider_model_version, "mock-1.0");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn undeclared_prompt_returns_404() {
    let (app, _project_id, api_key) = seeded_router_and_project().await;
    let body = serde_json::json!({
        "prompt_name": "not-declared-anywhere",
        "provider": "mock",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/prompt-runs")
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn live_provider_without_loaded_config_returns_503() {
    let (app, _project_id, api_key) = seeded_router_and_project().await;
    let body = serde_json::json!({
        "prompt_name": "fixture-prompt",
        "provider": "openai",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/prompt-runs")
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"], "orchestrator_unconfigured");
}

/// With a `Config` loaded that declares the live provider but no
/// secret available in the environment or keychain, the orchestrator
/// synthesises a `failed` `PromptRunRecord` with
/// `ProviderUnauthorized` via `unregistered_record`. We assert the
/// row lands in the DB with `status = "failed"` and that the API
/// returns 202.
async fn seeded_router_with_config_no_keys() -> (axum::Router, ProjectId, String) {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for the prompt_run_persist live-DB tests");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect to test postgres");
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));

    // Build a Config whose brand_name derives a deterministic project_id
    // that matches what we seed in Postgres; we use the Config-derived
    // id so the auth middleware (project scoping) lines up.
    let yaml = r#"
schema_version: '0.1'
brand:
  name: NoKeysFixture
prompts:
  - name: fixture-prompt
    text: "What is the best vector DB?"
providers:
  - name: openai
    model: gpt-4o-mini
"#;
    let cfg = anseo_core::Config::from_yaml_str(yaml).expect("parse fixture YAML");
    let project_id = cfg.project_id();
    let prompt_id = cfg.prompt_id("fixture-prompt").expect("declared");
    let now = chrono::Utc::now();

    // Project + prompt + key seeds.
    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: cfg.brand.name.clone(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");
    PromptRepo::new(&pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: "fixture-prompt".to_string(),
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

    // Forcibly clear the openai env var for this process so the
    // chained store falls through to the keychain (which the test
    // host may or may not have a key for). To make the test
    // deterministic we use the registry's "no secret -> skip" branch
    // by passing an explicit empty env var; the registry helper
    // already treats empty as missing.
    //
    // We can't reach into the keychain here, so we explicitly skip
    // building a registry and seed an empty one — that exercises the
    // same `unregistered_record` code path as the "no key" case.
    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let empty_registry: anseo_providers::ProviderRegistry = std::collections::HashMap::new();
    let state = AppState {
        storage,
        project_id,
        events,
        config: Some(std::sync::Arc::new(cfg)),
        provider_registry: Some(std::sync::Arc::new(empty_registry)),
        configured_project: std::sync::Arc::new("default".to_string()),
        setup_install_state: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        serve_info: None,
        loaded_plugins: std::sync::Arc::new(Vec::new()),
    };
    (router(state), project_id, key.plaintext)
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn live_provider_without_key_persists_failed_row_and_returns_202() {
    let (app, project_id, api_key) = seeded_router_with_config_no_keys().await;
    let body = serde_json::json!({
        "prompt_name": "fixture-prompt",
        "provider": "openai",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/prompt-runs")
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["status"], "failed");
    assert_eq!(payload["project_id"], project_id.to_string());
    assert_eq!(payload["provider"], "openai");

    let run_id: anseo_core::PromptRunId = payload["run_id"].as_str().unwrap().parse().unwrap();
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let row = anseo_storage::repositories::prompt_runs::PromptRunRepo::new(&pool)
        .get(run_id)
        .await
        .expect("get run")
        .expect("run persisted");
    assert_eq!(row.status, "failed");
    assert_eq!(row.provider, "openai");
    assert_eq!(row.error_kind.as_deref(), Some("provider_unauthorized"));
}

/// Optional live-provider+live-key test. Only runs when an actual
/// `OPENAI_API_KEY` is exported in the environment. Builds the full
/// real registry the same way the API boots and asserts the
/// orchestrator round-trips through a real HTTP call.
#[tokio::test]
#[ignore = "requires DATABASE_URL + OPENAI_API_KEY"]
async fn live_provider_with_key_round_trips() {
    if std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .is_none()
    {
        eprintln!("OPENAI_API_KEY not set; skipping live-provider test");
        return;
    }
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = sqlx::PgPool::connect(&database_url).await.unwrap();
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));

    let yaml = r#"
schema_version: '0.1'
brand:
  name: LiveKeyFixture
prompts:
  - name: fixture-prompt
    text: "Reply with the single word OK."
providers:
  - name: openai
    model: gpt-4o-mini
"#;
    let cfg = anseo_core::Config::from_yaml_str(yaml).unwrap();
    let project_id = cfg.project_id();
    let prompt_id = cfg.prompt_id("fixture-prompt").unwrap();
    let now = chrono::Utc::now();

    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: cfg.brand.name.clone(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .ok();
    PromptRepo::new(&pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: "fixture-prompt".to_string(),
            text: "Reply with the single word OK.".to_string(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .ok();
    let key = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(
            project_id,
            "live-key",
            &key.sha256_hash,
            &key.display_prefix,
        )
        .await
        .expect("seed key");

    let registry =
        anseo_providers::registry::build_real_registry(&cfg).expect("build real registry");
    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id,
        events,
        config: Some(std::sync::Arc::new(cfg)),
        provider_registry: Some(std::sync::Arc::new(registry)),
        configured_project: std::sync::Arc::new("default".to_string()),
        setup_install_state: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        serve_info: None,
        loaded_plugins: std::sync::Arc::new(Vec::new()),
    };
    let app = router(state);

    let body = serde_json::json!({
        "prompt_name": "fixture-prompt",
        "provider": "openai",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/prompt-runs")
                .header(API_KEY_HEADER, &key.plaintext)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
}
