//! Story 0.11 — `X-OpenGEO-Project` header substrate.
//!
//! Validates the Phase 2 contract end-to-end at the router level:
//!
//! - absent header → accepted (200), with a one-time WARN per process
//! - matching value (case-insensitive after trim) → accepted (200)
//! - `"default"` sentinel → accepted (200)
//! - mismatching value → **403** with `error_kind: "project_not_found"`
//!
//! To exercise the project guard without coupling to the auth gate (no
//! live DB), the tests build a minimal `Router` that mounts a trivial
//! handler under `/v1/*` and applies only `project_header_guard` as a
//! route layer. The same guard function is wired into the real
//! `/v1/*` surface from `apps/api/src/lib.rs`, so behavior is identical.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use opengeo_api::extractors::{project_header_guard, PROJECT_HEADER};
use opengeo_api::AppState;
use opengeo_core::ProjectId;
use tower::ServiceExt;

fn build_state(configured: &str) -> AppState {
    let lazy_pool = sqlx::PgPool::connect_lazy(
        "postgres://opengeo:opengeo@127.0.0.1:1/__project_header_test__",
    )
    .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(opengeo_storage::Storage::from_pool(lazy_pool));
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    AppState {
        storage,
        project_id: ProjectId::new(),
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new(configured.to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    }
}

fn build_router(configured: &str) -> Router {
    let state = build_state(configured);
    Router::new()
        .route("/v1/ping", get(|| async { "pong" }))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            project_header_guard,
        ))
        .with_state(state)
}

#[test]
fn header_name_matches_spec() {
    assert_eq!(PROJECT_HEADER, "X-OpenGEO-Project");
}

#[tokio::test]
async fn absent_header_passes_through() {
    let app = build_router("acme");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/ping")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn matching_value_passes_through() {
    let app = build_router("acme");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/ping")
                .header(PROJECT_HEADER, "acme")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn matching_value_is_case_insensitive() {
    let app = build_router("Acme");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/ping")
                .header(PROJECT_HEADER, "  ACME  ")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn default_sentinel_passes_through() {
    let app = build_router("acme");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/ping")
                .header(PROJECT_HEADER, "default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn mismatching_value_returns_403_with_project_not_found() {
    let app = build_router("acme");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/ping")
                .header(PROJECT_HEADER, "other-project")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error_kind"], "project_not_found");
}
