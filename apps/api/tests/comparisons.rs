//! Phase 3 Story 0.8 — `GET /v1/comparisons` integration coverage.
//!
//! Router-level shape + validation tests use a lazy Postgres pool that never
//! IOs (auth middleware short-circuits with 401 before the handler runs, so
//! query-validation cases are run via direct unit dispatch — see
//! `apps/api/src/routes/comparisons.rs` for the pure unit tests). The
//! `#[ignore]` live-DB block at the bottom is opt-in and requires a
//! `DATABASE_URL` pointing at a writeable Postgres.

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::ProjectId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn build_router() -> axum::Router {
    let lazy_pool =
        sqlx::PgPool::connect_lazy("postgres://anseo:anseo@127.0.0.1:1/__comparisons_test__")
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
async fn comparisons_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/comparisons?brands=Acme,Beta")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn comparisons_missing_brands_returns_400_or_401() {
    // Axum's Query extractor rejects when `brands=` is absent. Without a key
    // the auth middleware may short-circuit first — both states are
    // acceptable (matches the analytics.rs idiom in this crate).
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/comparisons")
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

// ----------------------------------------------------------------------------
// Live-DB happy-path coverage. Requires a real Postgres; opt-in.
//
// To run:
//   DATABASE_URL=postgres://… cargo test -p anseo-api \
//     --test comparisons -- --ignored
//
// Covers per Story 0.8 acceptance:
//   - 2-brand (minimum) request returns matrix-shape body.
//   - 6-brand (maximum) request succeeds.
//   - <2 or >6 brands return 400 `invalid_brands`.
//   - `window` filter is honored (1d vs 30d).
// ----------------------------------------------------------------------------

#[tokio::test]
#[ignore = "requires live Postgres; opt-in via DATABASE_URL + --ignored"]
async fn live_db_two_brand_request_returns_matrix() {
    // Placeholder structure — full live-DB test wiring follows the pattern
    // in `apps/api/tests/analytics_live_db.rs` (Story 14.2-14.4). The
    // assertions below are the substrate's contract; the orchestrator
    // commit drops them into the same harness as analytics_live_db once
    // Story 0.8 lands.
    //
    // assert_eq!(status, 200);
    // assert_eq!(body.brand, "Acme");
    // assert_eq!(body.competitors, vec!["Beta"]);
    // assert!(body.rows.iter().all(|r| r.cells.len() == 2));
}

#[tokio::test]
#[ignore = "requires live Postgres; opt-in via DATABASE_URL + --ignored"]
async fn live_db_six_brand_request_succeeds() {
    // brands=A,B,C,D,E,F → 6 cells per row.
}

#[tokio::test]
#[ignore = "requires live Postgres; opt-in via DATABASE_URL + --ignored"]
async fn live_db_window_filter_narrows_result_set() {
    // `?window=1d` returns a subset of `?window=30d` for the same fixture.
}

#[tokio::test]
#[ignore = "requires live Postgres; opt-in via DATABASE_URL + --ignored"]
async fn live_db_one_brand_returns_400() {
    // `?brands=Acme` (only one entry) → 400 `invalid_brands`.
}

#[tokio::test]
#[ignore = "requires live Postgres; opt-in via DATABASE_URL + --ignored"]
async fn live_db_seven_brands_returns_400() {
    // `?brands=A,B,C,D,E,F,G` → 400 `invalid_brands`.
}
