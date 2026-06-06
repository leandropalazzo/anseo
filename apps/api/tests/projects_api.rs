//! Story 36.3 — `/v1/projects` operator-scoped registry endpoints.
//!
//! Offline tests cover the auth seam (the operator surface is behind
//! `require_api_key` but NOT the project-header guard). Live-DB tests drive
//! list / create / get / archive end-to-end through the real router and are
//! `#[ignore]`d (run with `--ignored`):
//!
//! ```text
//! DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo_test \
//!   cargo test -p opengeo-api --test projects_api -- --ignored
//! ```

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use anseo_core::ProjectId;
use anseo_storage::repositories::{api_keys::ApiKeyRepo, projects::ProjectRepo};
use anseo_storage::Storage;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use sqlx::PgPool;
use tower::ServiceExt;

fn lazy_router() -> axum::Router {
    let lazy_pool =
        sqlx::PgPool::connect_lazy("postgres://opengeo:opengeo@127.0.0.1:1/__projects_api_test__")
            .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(Storage::from_pool(lazy_pool));
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
async fn list_projects_without_key_is_401() {
    let app = lazy_router();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/v1/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Operator surface is auth-gated. A malformed/absent key never reaches a
    // DB lookup (the lazy pool would otherwise error), so this is a clean 401.
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_project_without_key_is_401() {
    let app = lazy_router();
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/projects")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"acme"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// Live-DB end-to-end coverage
// ---------------------------------------------------------------------------

/// Seed an authenticated key against `home_project` and return the router + the
/// plaintext key header value. The key's project is irrelevant to the operator
/// surface (it's project-agnostic) but `require_api_key` needs a real hit.
async fn live_app() -> Option<(axum::Router, String)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Arc::new(Storage::from_pool(pool.clone()));
    storage.migrate().await.expect("migrate");

    // Clean slate so list assertions are deterministic.
    sqlx::query("UPDATE projects SET archived_at = now() WHERE archived_at IS NULL")
        .execute(&pool)
        .await
        .expect("reset");

    // A home project to own the API key.
    let home = ProjectId::new();
    ProjectRepo::new(&pool)
        .insert(&anseo_storage::models::ProjectRow {
            id: home,
            name: format!("home-{home}"),
            organization_id: None,
            tenant_id: None,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("seed home project");
    let key = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(home, "test", &key.sha256_hash, "ogeo_tst")
        .await
        .expect("seed key");

    // Re-archive the home project so it doesn't pollute the operator list /
    // sole-active math for the resolution-agnostic operator endpoints.
    ProjectRepo::new(&pool)
        .archive_project(home)
        .await
        .expect("archive home");

    let (events, _rx) = anseo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id: home,
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new("home".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        serve_info: None,
        loaded_plugins: std::sync::Arc::new(Vec::new()),
    };
    Some((router(state), key.plaintext))
}

fn req(method: &str, uri: &str, key: &str, body: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(API_KEY_HEADER, key);
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    b.body(
        body.map(|s| Body::from(s.to_string()))
            .unwrap_or(Body::empty()),
    )
    .unwrap()
}

async fn json(res: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn projects_crud_lifecycle() {
    let Some((app, key)) = live_app().await else {
        return;
    };
    let name = format!("Registry Co {}", uuid::Uuid::new_v4());

    // Empty to start (we archived the home project).
    let res = app
        .clone()
        .oneshot(req("GET", "/v1/projects", &key, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = json(res).await;
    assert_eq!(body["projects"].as_array().unwrap().len(), 0);

    // Create -> 201 + derived project_id.
    let res = app
        .clone()
        .oneshot(req(
            "POST",
            "/v1/projects",
            &key,
            Some(&format!(r#"{{"name":"{name}"}}"#)),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let created = json(res).await;
    let pid = created["project_id"].as_str().unwrap().to_string();
    assert_eq!(
        pid,
        anseo_core::project_id_for_name(&name).to_string(),
        "project_id must be derived from the brand name"
    );

    // Duplicate create -> 409.
    let res = app
        .clone()
        .oneshot(req(
            "POST",
            "/v1/projects",
            &key,
            Some(&format!(r#"{{"name":"{name}"}}"#)),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);

    // List now shows exactly the created project.
    let res = app
        .clone()
        .oneshot(req("GET", "/v1/projects", &key, None))
        .await
        .unwrap();
    let body = json(res).await;
    let arr = body["projects"].as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["project_id"], pid);

    // Get by id -> 200.
    let res = app
        .clone()
        .oneshot(req("GET", &format!("/v1/projects/{pid}"), &key, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json(res).await["name"], name);

    // Get unknown id -> 404.
    let other = ProjectId::new();
    let res = app
        .clone()
        .oneshot(req("GET", &format!("/v1/projects/{other}"), &key, None))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // Archive -> 204, then it drops from the list.
    let res = app
        .clone()
        .oneshot(req(
            "POST",
            &format!("/v1/projects/{pid}/archive"),
            &key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(req("GET", "/v1/projects", &key, None))
        .await
        .unwrap();
    assert_eq!(json(res).await["projects"].as_array().unwrap().len(), 0);

    // Archiving an unknown id -> 404.
    let res = app
        .oneshot(req(
            "POST",
            &format!("/v1/projects/{other}/archive"),
            &key,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
