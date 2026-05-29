//! P0-120 (test-design Epic 12) — `X-OpenGEO-API-Key` middleware coverage.
//!
//! Exercises the auth pipeline end-to-end at the router level: missing
//! header / wrong header / malformed token / valid-shaped-but-unknown all
//! return 401 before any database row is touched. A live-DB integration
//! test would also cover the happy path (key in DB → 200); that path
//! requires `DATABASE_URL` and is parked as a follow-up because the
//! Story 12.1 review session didn't have a live Postgres available.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use opengeo_api::{router, AppState};
use opengeo_core::api_key::API_KEY_HEADER;
use opengeo_core::ProjectId;
use tower::ServiceExt;

fn build_router() -> axum::Router {
    // Lazy pool that never touches the network. The auth middleware short-
    // circuits at the header/wire-shape check for malformed inputs, so the
    // pool is never queried in these tests.
    let lazy_pool = sqlx::PgPool::connect_lazy(
        "postgres://opengeo:opengeo@127.0.0.1:1/__auth_test__",
    )
    .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(opengeo_storage::Storage::from_pool(lazy_pool));
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id: ProjectId::new(),
        events,
        config: None,
        provider_registry: None,
        configured_project: std::sync::Arc::new("default".to_string()),
        setup_install_state: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
    };
    router(state)
}

#[tokio::test]
async fn v1_runs_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn v1_runs_with_wrong_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .header("Authorization", "Bearer ogeo_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Authorization header is the legacy/Bearer surface — spec mandates
    // X-OpenGEO-API-Key, so this request is treated as unauthenticated.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn v1_runs_with_malformed_token_returns_401_before_db() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .header(API_KEY_HEADER, "not-our-format")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // looks_like_key fails → 401, no DB lookup. (The lazy pool would error
    // on connect if we reached it.)
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_runs_root_path_also_returns_401_when_unauthenticated() {
    // Story 12.1 review Decision 3: root paths share the auth gate so a
    // public bind exposing /api/runs requires a valid header too.
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/runs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn x_opengeo_api_key_header_constant_is_what_we_send() {
    // Pin the spec wire name. If a future refactor renames the constant
    // away from `X-OpenGEO-API-Key`, this test fails immediately.
    assert_eq!(API_KEY_HEADER, "X-OpenGEO-API-Key");
}
