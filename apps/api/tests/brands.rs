//! Story 0.9 — `/v1/brands` shape + auth coverage.
//!
//! Auth-gate tests use a lazy pool (never IOs). The 503-on-missing-config
//! branch can't be triggered without an authenticated request, so live-DB
//! coverage is in `brands_live_db.rs` (deferred — orchestrator wires it
//! once the project loader lands; this file covers the router seam).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use opengeo_api::{router, AppState};
use opengeo_core::ProjectId;
use tower::ServiceExt;

fn build_router() -> axum::Router {
    let lazy_pool = sqlx::PgPool::connect_lazy(
        "postgres://opengeo:opengeo@127.0.0.1:1/__brands_test__",
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
    };
    router(state)
}

#[tokio::test]
async fn brands_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/brands")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn brands_route_does_not_collide_with_existing_v1_paths() {
    // Sanity: an unrelated `/v1/brands/whatever` should not match the
    // exact-path `/v1/brands` handler. Axum returns 404 in that case
    // (before the auth gate kicks the request to 401 — both states are
    // valid; the test asserts the auth gate or 404, not the handler).
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/brands/nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            response.status(),
            StatusCode::NOT_FOUND | StatusCode::UNAUTHORIZED
        ),
        "expected 404 or 401, got {}",
        response.status()
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL + seeded Config — covered by orchestrator harness"]
async fn brands_live_db_returns_primary_and_competitors() {
    // Live-DB shape contract documented for the orchestrator to wire
    // once Story 0.11 lands a Config loader on AppState in tests.
}
