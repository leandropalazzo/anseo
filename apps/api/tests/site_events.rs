//! Story 47.1 — `POST /v1/site-events` HTTP round-trip + privacy contract.
//!
//! Exercises the public (unauthenticated) ingest surface end-to-end through the
//! real router: silent-drop on unknown event types (AC-1), persistence with no
//! IP column (AC-2), rate-limiting (AC-3), the server-side `badge_embed_view`
//! insert on the badge endpoint (AC-4), and CORS acceptance (AC-8).
//!
//! Gated on a live DB and `#[ignore]`'d for the default run (CI runs them):
//!
//! ```text
//! DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo \
//!   cargo test -p anseo-api --test site_events -- --ignored
//! ```
//!
//! The rate-limiter is a process-global in-memory map keyed by a hash of the
//! request IP. To keep the AC-3 test independent of the others, it sends a
//! distinct `X-Forwarded-For` IP so it gets its own bucket.

use std::collections::HashMap;
use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::ProjectId;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn app() -> (axum::Router, sqlx::PgPool) {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for the site_events live-DB tests");
    let pool = sqlx::PgPool::connect(&database_url)
        .await
        .expect("connect to test postgres");
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));
    let project_name = format!("site-events-fixture-{}", ProjectId::new());
    let project_id = anseo_core::project_id_for_name(&project_name);
    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id,
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new(project_name),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        serve_info: None,
        loaded_plugins: Arc::new(Vec::new()),
            rate_limit: anseo_api::middleware::rate_limit::RateLimitStore::new(),
    };
    (router(state), pool)
}

fn post_event(body: serde_json::Value, forwarded_for: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/site-events")
        .header("content-type", "application/json")
        .header("origin", "https://anseo.ai")
        .header("x-forwarded-for", forwarded_for)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn valid_event_persists_and_unknown_is_silently_dropped() {
    let (app, pool) = app().await;
    let session = uuid::Uuid::new_v4();

    // AC-1: a valid payload returns 204 and persists (AC-2).
    let resp = app
        .clone()
        .oneshot(post_event(
            serde_json::json!({
                "event_type": "page_view",
                "session_id": session,
                "path": "/leaderboard",
                "referrer": "google.com",
                "properties": {"category": "vector-db"}
            }),
            "203.0.113.10",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // AC-1: an unknown event_type is also 204 (silent drop — no enumeration).
    let resp = app
        .clone()
        .oneshot(post_event(
            serde_json::json!({
                "event_type": "definitely_not_real",
                "session_id": session,
            }),
            "203.0.113.10",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // AC-2: the valid row landed; the unknown one did not.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM site_events WHERE session_id = $1")
        .bind(session)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 1, "only the valid page_view should persist");

    // AC-2: no IP column exists — selecting it must error.
    let probe = sqlx::query("SELECT ip FROM site_events LIMIT 1")
        .fetch_optional(&pool)
        .await;
    assert!(probe.is_err(), "site_events must NOT have an ip column");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn rate_limit_returns_429_past_window() {
    let (app, _pool) = app().await;
    // Unique IP so this test owns its rate-limit bucket.
    let ip = format!("198.51.100.{}", (std::process::id() % 250) + 1);
    let body = serde_json::json!({
        "event_type": "page_view",
        "session_id": uuid::Uuid::new_v4(),
    });

    // First 60 within the window succeed.
    for _ in 0..60 {
        let resp = app
            .clone()
            .oneshot(post_event(body.clone(), &ip))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }
    // The 61st is rate-limited.
    let resp = app
        .clone()
        .oneshot(post_event(body.clone(), &ip))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn badge_serve_inserts_badge_embed_view_event() {
    let (app, pool) = app().await;

    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM site_events WHERE event_type = 'badge_embed_view'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // AC-4: serving any badge (verified or lapsed) fires a server-side event.
    let req = Request::builder()
        .method("GET")
        .uri("/v1/badge/example.com/brand")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    // 200 (verified) or 410 (lapsed) — either way the event must fire.
    assert!(
        resp.status() == StatusCode::OK || resp.status() == StatusCode::GONE,
        "unexpected badge status: {}",
        resp.status()
    );

    let after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM site_events WHERE event_type = 'badge_embed_view'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after,
        before + 1,
        "badge serve must insert one badge_embed_view"
    );
}
