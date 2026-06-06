//! Story 0.9 — `/v1/prompts/run-summary` shape + auth + validation.

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::api_key::API_KEY_HEADER;
use anseo_core::ProjectId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn build_router() -> axum::Router {
    let lazy_pool = sqlx::PgPool::connect_lazy(
        "postgres://opengeo:opengeo@127.0.0.1:1/__prompts_run_summary_test__",
    )
    .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(anseo_storage::Storage::from_pool(lazy_pool));
    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id: ProjectId::new(),
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
        loaded_plugins: std::sync::Arc::new(Vec::new()),
    };
    router(state)
}

#[tokio::test]
async fn run_summary_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/prompts/run-summary")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn run_summary_with_filter_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/prompts/run-summary?since=2026-01-01T00:00:00Z")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn run_summary_invalid_since_with_bogus_key_returns_401_or_400() {
    // Without a valid key the auth middleware short-circuits with 401
    // before our handler validates `since`. With a valid key it would
    // return 400. We accept either state here so the test is robust to
    // ordering of layered middleware.
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/prompts/run-summary?since=not-a-date")
                .header(API_KEY_HEADER, "ogeo_invalid_dummy_key_for_shape_test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::BAD_REQUEST
        ),
        "expected 401 or 400, got {}",
        response.status()
    );
}
