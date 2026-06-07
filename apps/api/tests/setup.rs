//! Story 15.1 — Integration tests for `/v1/setup/*`.
//!
//! Three endpoints under test:
//!   - `GET  /v1/setup/status` — 200 with all six sections present.
//!   - `POST /v1/setup/clickhouse/install` — 202 with a ULID install_id +
//!     stream URL pointing at the SSE endpoint.
//!   - `GET  /v1/setup/clickhouse/install-stream?id=<ulid>` — emits ≥ 3
//!     SSE events for a mock install; unknown id → 404.
//!
//! Lazy-Postgres pattern (per `tests/analytics.rs`): we never IO to the
//! DB; sqlx `connect_lazy` defers connection until first query. The
//! `/setup/status` Postgres probe will time out (1s budget) and report
//! `state: "unknown"` — that is the correct, exercised behaviour.

use std::sync::Arc;
use std::time::Duration;

use anseo_api::{router, AppState};
use anseo_core::ProjectId;
use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

fn build_router() -> (axum::Router, ProjectId) {
    let lazy_pool = sqlx::PgPool::connect_lazy("postgres://anseo:anseo@127.0.0.1:1/__setup_test__")
        .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(anseo_storage::Storage::from_pool(lazy_pool));
    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let project_id = ProjectId::new();
    let state = AppState {
        storage,
        project_id,
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
        loaded_plugins: std::sync::Arc::new(Vec::new()),
    };
    (router(state), project_id)
}

/// `/v1/setup/*` is mounted inside the standard `/v1` auth gate. With
/// zero seeded API keys the gate returns 401 — we cannot exercise the
/// handlers through the full router without a live DB. Instead we mount
/// the `setup::v1_router()` standalone with the same `AppState` so the
/// handlers are reachable without the auth layer. This matches the
/// pattern used in `tests/project_header.rs` for the project-header
/// guard.
fn build_setup_only_router() -> axum::Router {
    let lazy_pool = sqlx::PgPool::connect_lazy("postgres://anseo:anseo@127.0.0.1:1/__setup_test__")
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
    axum::Router::new()
        .nest("/v1", anseo_api::routes::setup::v1_router())
        .with_state(state)
}

#[tokio::test]
async fn full_router_mounts_setup_routes_behind_auth_gate() {
    // Through the full router, `/v1/setup/status` without an API key →
    // 401 (handled by `require_api_key`). This pins that the routes
    // ARE mounted (a missing route would 404).
    let (app, _project_id) = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/setup/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn status_returns_200_with_all_sections() {
    let app = build_setup_only_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/setup/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // All six sections must be present even when individual probes fail.
    for key in [
        "postgres",
        "clickhouse",
        "worker",
        "webhook_target",
        "api_keys",
        "docker",
    ] {
        assert!(json.get(key).is_some(), "/status missing `{key}` section");
    }
    // Postgres probe will fail against the lazy pool (1s timeout); the
    // section must STILL appear with `state: "unknown"`.
    assert_eq!(json["postgres"]["state"], "unknown");
    // ClickHouse with no `CLICKHOUSE_URL` → not_configured.
    if std::env::var("CLICKHOUSE_URL").is_err() {
        assert_eq!(json["clickhouse"]["state"], "not_configured");
    }
    // api_keys is an array.
    assert!(json["api_keys"].is_array(), "api_keys must be an array");
}

#[tokio::test]
async fn install_returns_202_with_ulid_install_id() {
    let app = build_setup_only_router();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/setup/clickhouse/install")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let install_id = json["install_id"].as_str().unwrap();
    // Validate it parses as a ULID.
    ulid::Ulid::from_string(install_id).expect("install_id is a valid ULID");
    // Stream URL points at the SSE handler.
    let stream = json["stream"].as_str().unwrap();
    assert!(
        stream.starts_with("/v1/setup/clickhouse/install-stream?id="),
        "stream URL shape: {stream}"
    );
}

#[tokio::test]
async fn install_stream_emits_at_least_three_events_for_mock_install() {
    // We need to share state between the POST and the GET — that means
    // building a single Router and calling it twice with `clone()`.
    let app = build_setup_only_router();

    // Kick off the install.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/setup/clickhouse/install")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let install_id = json["install_id"].as_str().unwrap().to_string();

    // Now open the SSE stream and count events.
    let stream_resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/setup/clickhouse/install-stream?id={install_id}"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stream_resp.status(), StatusCode::OK);

    // Drain the body — SSE responses close when the source stream ends
    // (mock state machine reaches `complete`). The 7-step mock runs in
    // ~210ms; bound the read with a 5s timeout to surface deadlocks.
    let body = stream_resp.into_body();
    let bytes = tokio::time::timeout(Duration::from_secs(5), to_bytes(body, 256 * 1024))
        .await
        .expect("SSE drain timed out")
        .expect("SSE drain errored");
    let text = String::from_utf8_lossy(&bytes);
    // Each event is `event: install\ndata: {...}\n\n`. Count `event: install`
    // occurrences — must be ≥ 3 (the spec asks for ≥ 3; mock emits 7).
    let event_count = text.matches("event: install").count();
    assert!(
        event_count >= 3,
        "expected ≥ 3 install events, got {event_count}\n--- stream body ---\n{text}"
    );
}

#[tokio::test]
async fn install_stream_for_unknown_id_returns_404() {
    let app = build_setup_only_router();
    // A well-formed but never-issued ULID.
    let bogus = ulid::Ulid::new().to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/setup/clickhouse/install-stream?id={bogus}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn install_stream_with_malformed_id_returns_400() {
    let app = build_setup_only_router();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/setup/clickhouse/install-stream?id=not-a-ulid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Story 15.4 — POST /v1/setup/clickhouse/connect ──────────────────────────

async fn post_connect(
    app: axum::Router,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/setup/clickhouse/connect")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 16 * 1024).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn connect_rejects_non_http_endpoint_with_400() {
    let app = build_setup_only_router();
    let (status, json) = post_connect(
        app,
        serde_json::json!({ "endpoint": "ftp://nope.example", "preset": "custom" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["ok"], false);
    assert_eq!(json["state"], "bad_request");
}

#[tokio::test]
async fn connect_unreachable_endpoint_reports_unreachable() {
    // Port 1 on loopback is closed; curl returns "000" → Unreachable. The
    // handler returns 200 with a structured failure so the UI can render the
    // ErrorBanner copy. This exercises the probe path without a live DB.
    let app = build_setup_only_router();
    let (status, json) = post_connect(
        app,
        serde_json::json!({
            "endpoint": "http://127.0.0.1:1",
            "preset": "custom",
            "username": "u",
            "password": "p",
            "database": "default"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], false);
    assert_eq!(json["state"], "unreachable");
}
