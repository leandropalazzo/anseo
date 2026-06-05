//! Story 19.6 — `/v1/recommendations` generate / list / detail / transition
//! contract tests against a live Postgres.
//!
//! Covers:
//! - `POST /v1/recommendations/generate` returns **202** + a `status_url`
//!   (and **503** when no `Config` is loaded).
//! - `GET /v1/recommendations` returns cursor-paginated items + a `next_cursor`
//!   when more pages remain.
//! - `GET /v1/recommendations/:id` returns a seeded row and **404**s for a row
//!   owned by another project.
//! - `PATCH /v1/recommendations/:id/state` applies a legal transition and
//!   returns **409** for an illegal one.
//!
//! `#[ignore]`'d for the default cargo run; invoke with:
//!
//! ```text
//! DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo \
//!   cargo test -p opengeo-api --test recommendations_lifecycle -- --ignored
//! ```

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use opengeo_api::{router, AppState};
use opengeo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use opengeo_core::ProjectId;
use opengeo_storage::models::ProjectRow;
use opengeo_storage::repositories::recommendations::{NewRecommendation, RecommendationsRepo};
use opengeo_storage::repositories::{api_keys::ApiKeyRepo, projects::ProjectRepo};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

async fn pool() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for the recommendations_lifecycle live-DB tests");
    sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect to test postgres")
}

async fn seed_project(pool: &sqlx::PgPool) -> ProjectId {
    let project_id = ProjectId::new();
    ProjectRepo::new(pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("test-{}", project_id),
            organization_id: None,
            tenant_id: None,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("seed project");
    project_id
}

async fn seed_api_key(pool: &sqlx::PgPool, project_id: ProjectId) -> String {
    let key = gen_key();
    ApiKeyRepo::new(pool)
        .insert(
            project_id,
            "fixture-key",
            &key.sha256_hash,
            &key.display_prefix,
        )
        .await
        .expect("seed api key");
    key.plaintext
}

fn state_for(pool: &sqlx::PgPool, project_id: ProjectId) -> AppState {
    let storage = Arc::new(opengeo_storage::Storage::from_pool(pool.clone()));
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    AppState {
        storage,
        project_id,
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
    }
}

/// Seed an active recommendation in `generated` state and return its UUID.
async fn seed_recommendation(
    pool: &sqlx::PgPool,
    project_id: ProjectId,
    kind: &str,
    fingerprint: &str,
) -> Uuid {
    let id = Uuid::from_bytes(ulid::Ulid::new().to_bytes());
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    RecommendationsRepo::new(pool)
        .insert(NewRecommendation {
            id,
            project_id: project_uuid,
            kind: kind.to_string(),
            severity: "medium".to_string(),
            confidence_band: "high".to_string(),
            state: "generated".to_string(),
            summary: "seeded recommendation".to_string(),
            payload: json!({ "seed": true }),
            traceability: json!({ "input_fingerprint": fingerprint }),
            reproducibility_class: "deterministic".to_string(),
            reproducibility_note: None,
            tags: vec!["deterministic_lane".to_string()],
            input_fingerprint: fingerprint.to_string(),
            engine_version: "test-engine".to_string(),
            plugin_source: None,
        })
        .await
        .expect("seed recommendation")
        .expect("freshly inserted (no dedup)")
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn generate_without_config_returns_503() {
    let pool = pool().await;
    let project_id = seed_project(&pool).await;
    let api_key = seed_api_key(&pool, project_id).await;
    let app = router(state_for(&pool, project_id));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/recommendations/generate")
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let payload = body_json(response).await;
    assert_eq!(payload["error"], "config_unavailable");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn generate_with_empty_project_returns_202_and_status_url() {
    let pool = pool().await;
    // A Config-derived project with no prompts: the engine simply produces
    // nothing, but the endpoint must still accept the run per the async pattern.
    let yaml = r#"
schema_version: '0.1'
brand:
  name: RecGenFixture
prompts:
  - name: fixture-prompt
    text: "What is the best vector DB?"
providers:
  - name: openai
    model: gpt-4o-mini
"#;
    let cfg = opengeo_core::Config::from_yaml_str(yaml).expect("parse fixture YAML");
    let project_id = cfg.project_id();
    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: cfg.brand.name.clone(),
            organization_id: None,
            tenant_id: None,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("seed project");
    let api_key = seed_api_key(&pool, project_id).await;

    let storage = Arc::new(opengeo_storage::Storage::from_pool(pool.clone()));
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id,
        events,
        config: Some(Arc::new(cfg)),
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
    };
    let app = router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/recommendations/generate")
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let payload = body_json(response).await;
    assert_eq!(payload["status"], "generated");
    assert_eq!(payload["status_url"], "/v1/recommendations");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn list_paginates_with_cursor() {
    let pool = pool().await;
    let project_id = seed_project(&pool).await;
    let api_key = seed_api_key(&pool, project_id).await;
    // Three distinct active rows (distinct fingerprints dodge the dedup index).
    for i in 0..3 {
        seed_recommendation(&pool, project_id, "missing_brand_doc", &format!("fp-{i}")).await;
    }
    let app = router(state_for(&pool, project_id));

    // limit=2 -> first page has 2 items + a next_cursor.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/recommendations?limit=2")
                .header(API_KEY_HEADER, &api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page1 = body_json(response).await;
    assert_eq!(page1["items"].as_array().unwrap().len(), 2);
    let cursor = page1["next_cursor"].as_str().expect("next_cursor present");

    // Second page picks up the remaining row; no further cursor.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/recommendations?limit=2&cursor={cursor}"))
                .header(API_KEY_HEADER, &api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page2 = body_json(response).await;
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert!(page2["next_cursor"].is_null());
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn detail_returns_row_and_404s_cross_project() {
    let pool = pool().await;
    let project_id = seed_project(&pool).await;
    let other_project = seed_project(&pool).await;
    let api_key = seed_api_key(&pool, project_id).await;
    let rec_id = seed_recommendation(&pool, project_id, "missing_brand_doc", "fp-detail").await;
    // A row owned by `other_project` must be invisible to `project_id`'s key.
    let other_id = seed_recommendation(&pool, other_project, "missing_brand_doc", "fp-other").await;
    let app = router(state_for(&pool, project_id));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/recommendations/{rec_id}"))
                .header(API_KEY_HEADER, &api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let payload = body_json(response).await;
    assert_eq!(payload["kind"], "missing_brand_doc");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/recommendations/{other_id}"))
                .header(API_KEY_HEADER, &api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn transition_applies_legal_and_rejects_illegal() {
    let pool = pool().await;
    let project_id = seed_project(&pool).await;
    let api_key = seed_api_key(&pool, project_id).await;
    let rec_id = seed_recommendation(&pool, project_id, "missing_brand_doc", "fp-trans").await;
    let app = router(state_for(&pool, project_id));

    // generated -> surfaced is legal.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/recommendations/{rec_id}/state"))
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "to": "surfaced" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let payload = body_json(response).await;
    assert_eq!(payload["recommendation"]["state"], "surfaced");

    // surfaced -> measured is illegal (must go through acted) -> 409.
    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/v1/recommendations/{rec_id}/state"))
                .header(API_KEY_HEADER, &api_key)
                .header("content-type", "application/json")
                .body(Body::from(json!({ "to": "measured" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let payload = body_json(response).await;
    assert_eq!(payload["error"], "illegal_transition");
}
