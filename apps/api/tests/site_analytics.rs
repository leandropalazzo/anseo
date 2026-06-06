//! Story 47.4 — operator site-analytics dashboard read API.
//!
//! Two layers:
//!   * Route-shape / auth tests use a lazy Postgres pool that never IOs: the
//!     `require_api_key` layer short-circuits with 401 before the handler runs,
//!     so we assert both endpoints are wired onto the *operator* surface and
//!     gated (AC-6) without a live DB.
//!   * A `#[ignore]`d live-DB test seeds rollup rows and asserts the aggregation
//!     payloads (AC-1, AC-2). Run with:
//!     ```text
//!     DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo \
//!       cargo test -p anseo-api --test site_analytics -- --ignored
//!     ```

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use anseo_core::ProjectId;
use anseo_storage::repositories::api_keys::ApiKeyRepo;
use anseo_storage::repositories::projects::ProjectRepo;
use anseo_storage::models::ProjectRow;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use sqlx::PgPool;
use tower::ServiceExt;

fn lazy_state() -> AppState {
    let lazy_pool =
        sqlx::PgPool::connect_lazy("postgres://opengeo:opengeo@127.0.0.1:1/__site_analytics_test__")
            .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(anseo_storage::Storage::from_pool(lazy_pool));
    let (events, _rx) = anseo_scheduler::worker::event_channel();
    AppState {
        storage,
        project_id: ProjectId::new(),
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
        loaded_plugins: Arc::new(Vec::new()),
    }
}

#[tokio::test]
async fn site_overview_requires_auth() {
    // Operator surface: no key ⇒ 401, before any DB IO.
    let app = router(lazy_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/analytics/site-overview?period=7d")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn funnels_requires_auth() {
    let app = router(lazy_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/analytics/funnels")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────────────
// Live-DB aggregation happy path (AC-1, AC-2). #[ignore] so default runs stay
// offline.
// ─────────────────────────────────────────────────────────────────────────────

async fn seed_and_route() -> (axum::Router, String, PgPool) {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for the site_analytics live test");
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));
    let project_id = ProjectId::new();

    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("test-{project_id}"),
            organization_id: None,
            tenant_id: None,
            created_at: Utc::now(),
        })
        .await
        .expect("seed project");

    // Seed raw site events, then compute rollups (exactly what the worker does).
    let repo = storage.site_events();
    let s1 = uuid::Uuid::new_v4();
    let s2 = uuid::Uuid::new_v4();
    for (et, path, referrer, props) in [
        ("page_view", Some("/"), Some("google.com"), serde_json::json!({})),
        ("page_view", Some("/leaderboard"), None, serde_json::json!({})),
        ("contribute_start", None, None, serde_json::json!({})),
        ("contribute_step", None, None, serde_json::json!({"step": "consent"})),
        ("contribute_complete", None, None, serde_json::json!({})),
        ("verify_start", None, None, serde_json::json!({"method": "dns"})),
        ("verify_complete", None, None, serde_json::json!({"method": "dns"})),
        ("badge_embed_view", None, None, serde_json::json!({})),
    ] {
        let sid = if et == "page_view" { s1 } else { s2 };
        repo.insert(et, sid, path, referrer, &props).await.expect("insert");
    }
    repo.compute_rollups().await.expect("rollup");

    let key = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(project_id, "fixture-key", &key.sha256_hash, &key.display_prefix)
        .await
        .expect("seed api key");

    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id,
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
        loaded_plugins: Arc::new(Vec::new()),
    };
    (router(state), key.plaintext, pool)
}

async fn get_json(app: &axum::Router, uri: &str, key: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(API_KEY_HEADER, key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 256 * 1024).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (live Postgres)"]
async fn site_overview_and_funnels_aggregate_correctly() {
    let (app, key, _pool) = seed_and_route().await;

    let (status, overview) = get_json(&app, "/v1/analytics/site-overview?period=7d", &key).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(overview["period_days"], 7);
    // "/" and "/leaderboard" both seen once today.
    let pages = overview["top_pages"].as_array().unwrap();
    assert!(pages.iter().any(|p| p["path"] == "/" && p["views"] == 1));
    assert!(overview["top_referrers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["domain"] == "google.com"));

    let (status, funnels) = get_json(&app, "/v1/analytics/funnels", &key).await;
    assert_eq!(status, StatusCode::OK);
    let contribute = funnels["contribute"].as_array().unwrap();
    assert_eq!(contribute[0]["label"], "contribute_start");
    assert_eq!(contribute[0]["count"], 1);
    // verify dns method present with start+complete.
    let verify = funnels["verify"].as_array().unwrap();
    assert!(verify.iter().any(|v| v["method"] == "dns" && v["start"] == 1 && v["complete"] == 1));
    // badge embeds today = 1.
    assert_eq!(
        funnels["badge_embeds_per_day"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["count"].as_i64().unwrap())
            .sum::<i64>(),
        1
    );
}
