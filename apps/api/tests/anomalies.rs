//! Phase 3 Story 0.7 — `/v1/anomalies` substrate endpoint contract tests.
//!
//! Router-level shape coverage mirrors `analytics.rs`:
//! - Missing `X-OpenGEO-API-Key` → 401.
//! - Unknown enum variants for `window` / `kind` → 400 (axum `Query` rejects
//!   before the auth gate is reached for some routings; both 400 and 401 are
//!   acceptable per Phase 2 precedent).
//! - The `X-Anseo-Project` header is accepted without 4xx (Story 0.11 L2:
//!   accepted-but-ignored).
//!
//! Live-DB happy paths sit in the orchestrator's `*_live_db.rs` companion
//! files; this one runs against a lazy pool that never IOs. The `#[ignore]`
//! marker on the round-trip enum test is documented inline.

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::ProjectId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn build_router() -> axum::Router {
    let lazy_pool =
        sqlx::PgPool::connect_lazy("postgres://opengeo:opengeo@127.0.0.1:1/__anomalies_test__")
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
async fn anomalies_without_header_returns_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/anomalies")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn anomalies_with_unknown_kind_returns_400_or_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/anomalies?kind=bogus")
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

#[tokio::test]
async fn anomalies_with_unknown_window_returns_400_or_401() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/anomalies?window=99d")
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

#[tokio::test]
async fn anomalies_with_project_header_does_not_400() {
    // Story 0.11 L2: the header is accepted but does not gate. With no
    // API key we still expect a 401 — the assertion is that the
    // unknown-header path doesn't degrade to a 400.
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/anomalies")
                .header("X-Anseo-Project", "default")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Deserialization round-trip pin for the wire enums. The `/v1/anomalies`
/// items use `snake_case` for `kind` and `severity`; downstream consumers
/// (the `list_trends` MCP tool in epic 16) pattern-match on these strings,
/// so a silent rename here would break them. Pinning the variant strings
/// at compile time on this side keeps the contract honest.
#[test]
fn anomaly_item_wire_strings_pinned() {
    use anseo_api::routes::anomalies::{AnomalyItemKind, AnomalySeverity};
    assert_eq!(
        serde_json::to_string(&AnomalyItemKind::VisibilityDrop).unwrap(),
        "\"visibility_drop\""
    );
    assert_eq!(
        serde_json::to_string(&AnomalyItemKind::CitationLoss).unwrap(),
        "\"citation_loss\""
    );
    assert_eq!(
        serde_json::to_string(&AnomalyItemKind::RankSwap).unwrap(),
        "\"rank_swap\""
    );
    assert_eq!(
        serde_json::to_string(&AnomalySeverity::Low).unwrap(),
        "\"low\""
    );
    assert_eq!(
        serde_json::to_string(&AnomalySeverity::Medium).unwrap(),
        "\"medium\""
    );
    assert_eq!(
        serde_json::to_string(&AnomalySeverity::High).unwrap(),
        "\"high\""
    );
}

/// Live-DB happy path. Marked `#[ignore]` so the offline test suite skips
/// it; the orchestrator runs it as part of the live-DB sweep with
/// `cargo test -- --ignored`.
#[tokio::test]
#[ignore = "requires live Postgres + seeded webhook_deliveries; orchestrator runs --ignored"]
async fn anomalies_live_db_filters_empty_result() {
    // Placeholder — Story 0.7 substrate ships with the contract-level
    // tests above; a live-DB happy path is wired by the orchestrator's
    // sibling `anomalies_live_db.rs` once a seed harness for anomaly
    // events lands.
}
