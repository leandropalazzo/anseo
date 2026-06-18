//! Story 40.1 — `POST /v1/ingest/run` HTTP round-trip (live Postgres).
//!
//! These exercise the full HTTP path: the `X-Anseo-Project` guard resolving
//! the scope, the prompt-declared check, and the prompt_run persist. The
//! gate-critical consent/KEK decision logic (opted+KEK → sealed,
//! opted+no-KEK → flagged, stale terms → rejected) is covered exhaustively by
//! the always-run unit tests in `routes::ingest` (`decide_contribution`), which
//! need neither a DB nor a keyring; this file pins the surface around them.
//!
//! Gated on a live DB and `#[ignore]`'d for the default run. Invoke with:
//!
//! ```text
//! DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo \
//!   cargo test -p anseo-api --test ingest -- --ignored
//! ```

use std::sync::Arc;

use anseo_api::extractors::project::PROJECT_HEADER;
use anseo_api::{router, AppState};
use anseo_benchmark::{CryptoError, ProjectKek, SealedContribution, TERMS_VERSION};
use anseo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use anseo_core::ProjectId;
use anseo_core::{AnomalySensitivity, BrandConfig, Config, PromptConfig, SCHEMA_VERSION_V0_2};
use anseo_storage::models::{ProjectRow, PromptRow};
use anseo_storage::repositories::{
    anonymous_contributions::AnonymousContributionRepo, api_keys::ApiKeyRepo,
    benchmark_consent::BenchmarkConsentRepo, citations::CitationRepo, mentions::MentionRepo,
    projects::ProjectRepo, prompts::PromptRepo,
};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

/// Seed a uniquely-named project + a declared prompt + an api key, and return
/// the wired router plus the resolution handles. The project NAME is what the
/// `X-Anseo-Project` header resolves against, so it must be unique per run.
async fn seeded() -> (axum::Router, ProjectId, String, String) {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for the ingest live-DB tests");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect to test postgres");
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));
    storage.migrate().await.expect("apply storage migrations");

    // Derive the project id from its name so the header resolver
    // (`project_id_for_name`) lands on the row we seed.
    let project_name = format!("ingest-fixture-{}", ProjectId::new());
    let project_id = anseo_core::project_id_for_name(&project_name);
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
            id: anseo_core::PromptId::new(),
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

    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let config = Config {
        schema_version: SCHEMA_VERSION_V0_2.to_string(),
        brand: BrandConfig {
            name: "Pinecone".to_string(),
            variants: vec!["pinecone".to_string()],
            site_url: None,
        },
        competitors: Vec::new(),
        prompts: vec![PromptConfig {
            name: "vector-db".to_string(),
            text: "What is the best vector DB?".to_string(),
            description: Some("fixture".to_string()),
        }],
        providers: Vec::new(),
        schedules: Vec::new(),
        concurrency: 4,
        anomaly_sensitivity: AnomalySensitivity::default(),
        analytics: None,
    };
    let state = AppState {
        storage,
        project_id,
        events,
        config: Some(Arc::new(config)),
        provider_registry: None,
        configured_project: Arc::new(project_name.clone()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
        loaded_plugins: std::sync::Arc::new(Vec::new()),
            rate_limit: anseo_api::middleware::rate_limit::RateLimitStore::new(),
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

fn get(uri: &str, key: &str, project_name: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header(API_KEY_HEADER, key)
        .header(PROJECT_HEADER, project_name)
        .body(Body::empty())
        .unwrap()
}

async fn seed_anonymous_optin(project_id: ProjectId) -> uuid::Uuid {
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    BenchmarkConsentRepo::new(&pool)
        .record_optin(
            project_id,
            TERMS_VERSION,
            Some("test"),
            Some("ingest live-db"),
        )
        .await
        .expect("seed anonymous benchmark opt-in")
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_scopes_to_resolved_project_and_persists() {
    let (app, project_id, project_name, api_key) = seeded().await;

    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "raw_response": {
            "id": "resp_123",
            "text": "Pinecone, see https://docs.pinecone.io/guide"
        },
        "metadata": {
            "sdk": "python",
            "trace_id": "trace-123"
        },
        "observed_rank": 2,
    });
    let response = app
        .clone()
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
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
    let run_id: anseo_core::PromptRunId = payload["run_id"].as_str().unwrap().parse().unwrap();
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let row = anseo_storage::repositories::prompt_runs::PromptRunRepo::new(&pool)
        .get(run_id)
        .await
        .expect("get run")
        .expect("run persisted");
    assert_eq!(row.provider, "openai");
    assert_eq!(row.provider_model_version, "gpt-4o-2024-08-06");
    assert_eq!(row.status, "ok");
    assert_eq!(row.raw_response["id"], "resp_123");
    assert_eq!(row.request_parameters["metadata"]["sdk"], "python");
    assert_eq!(row.request_parameters["metadata"]["trace_id"], "trace-123");

    let detail = app
        .clone()
        .oneshot(get(&format!("/v1/runs/{run_id}"), &api_key, &project_name))
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_bytes = axum::body::to_bytes(detail.into_body(), 64 * 1024)
        .await
        .unwrap();
    let detail_payload: serde_json::Value = serde_json::from_slice(&detail_bytes).unwrap();
    assert_eq!(detail_payload["id"], run_id.to_string());
    assert_eq!(detail_payload["status"], "ok");
    assert_eq!(detail_payload["raw_response"]["id"], "resp_123");

    let mentions = MentionRepo::new(&pool)
        .list_by_run(run_id)
        .await
        .expect("mentions by run");
    assert!(
        !mentions.is_empty(),
        "ingest should exercise the extraction persistence path when config is present"
    );
    let citations = CitationRepo::new(&pool)
        .list_by_run(run_id)
        .await
        .expect("citations by run");
    assert!(
        citations
            .iter()
            .any(|citation| citation.domain == "docs.pinecone.io"),
        "expected extracted citation domain from canonical raw_response text"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_contribute_false_writes_no_contribution() {
    // Story 40.4 AC-2/AC-6: contribute defaults to false ⇒ the run is recorded
    // but the contribution leg is an explicit skip, never a sealed row. (No
    // benchmark opt-in on this fresh project, and no `contribute` field, so the
    // narrower-of-two-gates is closed.)
    let (app, _project_id, project_name, api_key) = seeded().await;
    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "response_text": "Pinecone, see https://docs.pinecone.io/guide",
        "contribute": false,
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // contribute=false ⇒ skipped, never "sealed".
    assert_eq!(payload["contribution"]["status"], "skipped_not_opted_in");
    assert_ne!(payload["contribution"]["status"], "sealed");

    let run_id: anseo_core::PromptRunId = payload["run_id"].as_str().unwrap().parse().unwrap();
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let row = AnonymousContributionRepo::new(&pool)
        .by_prompt_run(run_id)
        .await
        .expect("query anonymous contributions");
    assert!(
        row.is_none(),
        "contribute=false must not persist a benchmark row"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_contribute_true_without_kek_is_rejected() {
    // Story 40.4 AC-1 hard gate: a `contribute: true` request on a project with
    // no per-project benchmark KEK is rejected up-front (403 kek_missing) — the
    // run is NOT recorded under a false promise of contribution.
    let (app, project_id, project_name, api_key) = seeded().await;
    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "response_text": "Pinecone",
        "contribute": true,
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"], "kek_missing");

    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM anonymous_contributions WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        count, 0,
        "403 contribute=true must not persist a benchmark row"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_contribute_true_persists_sealed_anonymous_contribution() {
    let (app, project_id, project_name, api_key) = seeded().await;
    let store = anseo_core::default_chain();
    let project_id_str = project_id.to_string();
    let _kek = ProjectKek::load_or_create(&store, &project_id_str).unwrap();
    let consent_id = seed_anonymous_optin(project_id).await;

    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "response_text": "Pinecone, see https://docs.pinecone.io/guide",
        "contribute": true,
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["contribution"]["status"], "sealed");

    let run_id: anseo_core::PromptRunId = payload["run_id"].as_str().unwrap().parse().unwrap();
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let row = AnonymousContributionRepo::new(&pool)
        .by_prompt_run(run_id)
        .await
        .expect("read contribution row")
        .expect("sealed contribution persisted");
    assert_eq!(row.project_id, project_id);
    assert_eq!(row.consent_record_id, consent_id);
    assert_eq!(row.terms_version, TERMS_VERSION);

    let sealed: SealedContribution = serde_json::from_value(row.sealed_payload).unwrap();
    let loaded = ProjectKek::load(&store, &project_id_str).unwrap();
    let opened = loaded.open(&sealed).expect("open persisted sealed payload");
    assert_eq!(opened.prompt_slug(), "vector-db");

    ProjectKek::destroy(&store, &project_id_str).unwrap();
    let err = ProjectKek::load(&store, &project_id_str).unwrap_err();
    assert!(matches!(err, CryptoError::KekMissing { .. }));
    let new_kek = ProjectKek::load_or_create(&store, &project_id_str).unwrap();
    assert!(
        new_kek.open(&sealed).is_err(),
        "persisted sealed payload must stay undecryptable after shred + rekey"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_undeclared_prompt_returns_422() {
    let (app, _project_id, project_name, api_key) = seeded().await;
    let body = serde_json::json!({
        "prompt_slug": "not-declared",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "raw_response": "hello",
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"], "prompt_not_found");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_unknown_provider_returns_422() {
    let (app, _project_id, project_name, api_key) = seeded().await;
    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "totally-made-up",
        "model": "gpt-4o-2024-08-06",
        "raw_response": "hello",
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"], "provider_not_supported");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_explicit_citation_domains_persist_without_text() {
    let (app, _project_id, project_name, api_key) = seeded().await;
    let body = serde_json::json!({
        "prompt_slug": "vector-db",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "raw_response": { "id": "resp_annotations_only" },
        "citation_domains": ["docs.pinecone.io", "community.pinecone.io"],
    });
    let response = app
        .oneshot(post("/v1/ingest/run", &api_key, &project_name, body))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let run_id = payload["run_id"].as_str().unwrap().parse().unwrap();

    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap())
        .await
        .unwrap();
    let citations = CitationRepo::new(&pool)
        .list_by_run(run_id)
        .await
        .expect("citations by run");
    assert!(
        citations
            .iter()
            .any(|citation| citation.domain == "docs.pinecone.io"),
        "expected explicit citation domain to persist when no extractable text is present"
    );
    assert!(
        citations
            .iter()
            .any(|citation| citation.domain == "community.pinecone.io"),
        "expected all explicit citation domains to persist"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn ingest_rate_limit_returns_429() {
    let (app, _project_id, project_name, api_key) = seeded().await;
    for _ in 0..60 {
        let response = app
            .clone()
            .oneshot(post(
                "/v1/ingest/run",
                &api_key,
                &project_name,
                serde_json::json!({
                    "prompt_slug": "vector-db",
                    "provider": "openai",
                    "model": "gpt-4o-2024-08-06",
                    "raw_response": "Pinecone",
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
    }

    let response = app
        .oneshot(post(
            "/v1/ingest/run",
            &api_key,
            &project_name,
            serde_json::json!({
                "prompt_slug": "vector-db",
                "provider": "openai",
                "model": "gpt-4o-2024-08-06",
                "raw_response": "Pinecone",
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["error"], "rate_limited");
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
