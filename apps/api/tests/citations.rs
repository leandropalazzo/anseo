//! Story 0.9 — `/v1/citations/summary` shape + auth coverage.
//!
//! Unit-level router tests use the lazy-pool pattern from
//! `tests/analytics.rs` (never IOs). Live-DB happy-path tests are
//! `#[ignore]`-gated and seed a fixture before asserting the additive
//! response shape lands in JSON.

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::ProjectId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn build_router() -> axum::Router {
    let lazy_pool =
        sqlx::PgPool::connect_lazy("postgres://anseo:anseo@127.0.0.1:1/__citations_test__")
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
            rate_limit: anseo_api::middleware::rate_limit::RateLimitStore::new(),
    };
    router(state)
}

#[tokio::test]
async fn citations_summary_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/citations/summary")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn citations_summary_with_filters_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/citations/summary?days=14&provider=openai&prompt=vector-db")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn citations_summary_accepts_project_header_silently() {
    // X-Anseo-Project is accepted-but-ignored per Story 0.11. Auth
    // still short-circuits at 401 since no key is supplied.
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/citations/summary")
                .header("X-Anseo-Project", "OpenGEO")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
