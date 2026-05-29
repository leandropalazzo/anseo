//! Story 10.4 — schedules write surface round-trip against live Postgres.
//!
//! Covers:
//! - POST /v1/schedules happy path (returns 201 with schedule + projection).
//! - POST with duplicate name returns 409.
//! - POST with unknown provider returns 400 invalid_provider.
//! - PUT toggles paused.
//! - DELETE returns 204; subsequent GET returns 404.
//!
//! Gated `#[ignore]`; run with
//! `DATABASE_URL=... cargo test -p opengeo-api --test schedules_live_db -- --ignored`.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use opengeo_api::{router, AppState};
use opengeo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use opengeo_core::ProjectId;
use opengeo_storage::models::ProjectRow;
use opengeo_storage::repositories::{api_keys::ApiKeyRepo, projects::ProjectRepo};
use sqlx::PgPool;
use tower::ServiceExt;

async fn seed() -> (axum::Router, String) {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for schedules_live_db");
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Arc::new(opengeo_storage::Storage::from_pool(pool.clone()));
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
    let key = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(project_id, "fixture-key", &key.sha256_hash, &key.display_prefix)
        .await
        .expect("seed key");
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
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
    };
    (router(state), key.plaintext)
}

async fn req(
    app: &axum::Router,
    method: Method,
    uri: &str,
    api_key: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(API_KEY_HEADER, api_key);
    let body = match body {
        Some(j) => {
            builder = builder.header("content-type", "application/json");
            Body::from(j.to_string())
        }
        None => Body::empty(),
    };
    let response = app.clone().oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 256 * 1024).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn create_get_update_delete_round_trip() {
    let (app, key) = seed().await;

    let (create_status, created) = req(
        &app,
        Method::POST,
        "/v1/schedules",
        &key,
        Some(serde_json::json!({
            "name": "daily-mock",
            "cron": "daily",
            "prompts": ["vector-db"],
            "providers": ["openai"],
            "debounce_minutes": 10,
        })),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED, "create response: {created}");
    assert_eq!(created["name"], "daily-mock");
    assert_eq!(created["cron"], "daily");
    assert_eq!(created["paused"], false);
    assert!(created["projected_monthly_usd"].is_number(), "expected projected_monthly_usd");
    let id = created["id"].as_str().expect("id is string").to_string();

    let (get_status, got) = req(&app, Method::GET, &format!("/v1/schedules/{id}"), &key, None).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(got["id"], id);

    let (pause_status, paused) = req(
        &app,
        Method::PUT,
        &format!("/v1/schedules/{id}"),
        &key,
        Some(serde_json::json!({ "paused": true })),
    )
    .await;
    assert_eq!(pause_status, StatusCode::OK);
    assert_eq!(paused["paused"], true);

    let (del_status, _) = req(&app, Method::DELETE, &format!("/v1/schedules/{id}"), &key, None).await;
    assert_eq!(del_status, StatusCode::NO_CONTENT);

    let (after_get_status, _) = req(&app, Method::GET, &format!("/v1/schedules/{id}"), &key, None).await;
    assert_eq!(after_get_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn duplicate_name_returns_409() {
    let (app, key) = seed().await;
    let body = serde_json::json!({
        "name": "dup",
        "cron": "daily",
        "prompts": ["vector-db"],
        "providers": ["openai"],
    });
    let (first, _) = req(&app, Method::POST, "/v1/schedules", &key, Some(body.clone())).await;
    assert_eq!(first, StatusCode::CREATED);
    let (second, payload) = req(&app, Method::POST, "/v1/schedules", &key, Some(body)).await;
    assert_eq!(second, StatusCode::CONFLICT);
    assert_eq!(payload["error"], "duplicate_schedule");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn unknown_provider_returns_400() {
    let (app, key) = seed().await;
    let (status, payload) = req(
        &app,
        Method::POST,
        "/v1/schedules",
        &key,
        Some(serde_json::json!({
            "name": "bad-provider",
            "cron": "daily",
            "prompts": ["vector-db"],
            "providers": ["definitely-not-real"],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(payload["error"], "invalid_provider");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn unsupported_cadence_returns_400() {
    let (app, key) = seed().await;
    let (status, payload) = req(
        &app,
        Method::POST,
        "/v1/schedules",
        &key,
        Some(serde_json::json!({
            "name": "bad-cron",
            "cron": "every 7 fortnights",
            "prompts": ["vector-db"],
            "providers": ["openai"],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(payload["error"], "unsupported_cadence");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn update_with_no_fields_returns_400() {
    let (app, key) = seed().await;
    let (create_status, created) = req(
        &app,
        Method::POST,
        "/v1/schedules",
        &key,
        Some(serde_json::json!({
            "name": "no-op-target",
            "cron": "daily",
            "prompts": ["vector-db"],
            "providers": ["openai"],
        })),
    )
    .await;
    assert_eq!(create_status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    let (status, payload) = req(
        &app,
        Method::PUT,
        &format!("/v1/schedules/{id}"),
        &key,
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(payload["error"], "no_op");
}
