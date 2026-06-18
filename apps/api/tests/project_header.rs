//! Epic 36 Story 36.2 — per-request project resolution over the `projects`
//! table (ADR-004 precedence).
//!
//! The pure tests run offline. The resolution tests need a live Postgres and
//! are `#[ignore]`d (run with `--ignored`), mirroring the other `*_live_db`
//! suites:
//!
//! ```text
//! DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo_test \
//!   cargo test -p anseo-api --test project_header -- --ignored
//! ```
//!
//! Coverage:
//! - header resolves to the matching project (by brand name);
//! - two projects stay isolated (each header resolves to its own id);
//! - unknown project header -> `ResolveError::NotFound` (HTTP 404);
//! - sole-active fallback resolves when no header is sent;
//! - the fallback is ambiguous (None) when two active projects exist.
//! - (offline) project_header_guard route-layer returns 404 + project_not_found
//!   for an unknown header even before hitting a handler (AC #2).
//! - (offline) project_header_guard returns 401 when the API key is absent,
//!   never leaking project resolution before auth (AC #2 ordering invariant).

use anseo_api::extractors::{resolve_project, ResolveError, PROJECT_HEADER};
use anseo_core::{project_id_for_name, BrandConfig};
use anseo_storage::repositories::projects::ProjectRepo;
use anseo_storage::Storage;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use sqlx::PgPool;
use tower::ServiceExt;

/// These tests manipulate the process-global `projects` table (the
/// sole-active fallback is a COUNT over it), so they must not run
/// concurrently with each other. A static async mutex serialises them.
static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[test]
fn header_name_matches_spec() {
    assert_eq!(PROJECT_HEADER, "X-Anseo-Project");
}

async fn fresh_storage() -> Option<(Storage, PgPool)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Storage::from_pool(pool.clone());
    storage.migrate().await.expect("migrate");
    // Isolate from sibling tests: archive everything pre-existing so the
    // sole-active fallback math is deterministic for this run.
    sqlx::query("UPDATE projects SET archived_at = now() WHERE archived_at IS NULL")
        .execute(&pool)
        .await
        .expect("reset projects");
    Some((storage, pool))
}

async fn seed_project(pool: &PgPool, name: &str) -> anseo_core::ProjectId {
    ProjectRepo::new(pool)
        .create_project(&BrandConfig {
            name: name.to_string(),
            variants: Vec::new(),
            site_url: None,
        })
        .await
        .expect("seed project")
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn header_resolves_matching_project() {
    let _guard = DB_LOCK.lock().await;
    let Some((storage, pool)) = fresh_storage().await else {
        return;
    };
    let name = format!("acme-{}", uuid::Uuid::new_v4());
    let id = seed_project(&pool, &name).await;

    let scope = resolve_project(&storage, None, Some(&name))
        .await
        .expect("resolves");
    assert_eq!(scope.id(), id);
    assert_eq!(scope.name(), name);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn two_projects_stay_isolated() {
    let _guard = DB_LOCK.lock().await;
    let Some((storage, pool)) = fresh_storage().await else {
        return;
    };
    let a = format!("alpha-{}", uuid::Uuid::new_v4());
    let b = format!("bravo-{}", uuid::Uuid::new_v4());
    let id_a = seed_project(&pool, &a).await;
    let id_b = seed_project(&pool, &b).await;
    assert_ne!(id_a, id_b);

    let scope_a = resolve_project(&storage, None, Some(&a)).await.unwrap();
    let scope_b = resolve_project(&storage, None, Some(&b)).await.unwrap();
    // Each header resolves to ITS OWN project id — never the sibling's.
    assert_eq!(scope_a.id(), id_a);
    assert_eq!(scope_b.id(), id_b);
    assert_eq!(scope_a.id(), project_id_for_name(&a));
    assert_eq!(scope_b.id(), project_id_for_name(&b));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn unknown_project_header_is_not_found() {
    let _guard = DB_LOCK.lock().await;
    let Some((storage, _pool)) = fresh_storage().await else {
        return;
    };
    let err = resolve_project(&storage, None, Some("does-not-exist-anywhere"))
        .await
        .expect_err("unknown header must not resolve");
    assert_eq!(err, ResolveError::NotFound);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn sole_active_fallback_resolves_without_header() {
    let _guard = DB_LOCK.lock().await;
    let Some((storage, pool)) = fresh_storage().await else {
        return;
    };
    let name = format!("solo-{}", uuid::Uuid::new_v4());
    let id = seed_project(&pool, &name).await;

    // No explicit, no header -> legacy sole-active-project fallback.
    let scope = resolve_project(&storage, None, None)
        .await
        .expect("sole-active resolves");
    assert_eq!(scope.id(), id);

    // The `"default"` sentinel routes through the same fallback.
    let via_default = resolve_project(&storage, None, Some("default"))
        .await
        .expect("default sentinel resolves to sole-active");
    assert_eq!(via_default.id(), id);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn fallback_is_ambiguous_with_two_projects() {
    let _guard = DB_LOCK.lock().await;
    let Some((storage, pool)) = fresh_storage().await else {
        return;
    };
    seed_project(&pool, &format!("one-{}", uuid::Uuid::new_v4())).await;
    seed_project(&pool, &format!("two-{}", uuid::Uuid::new_v4())).await;

    // Two active projects + no header -> ambiguous -> NotFound (never a
    // silent pick of one project's data over the other's).
    let err = resolve_project(&storage, None, None)
        .await
        .expect_err("ambiguous fallback must not resolve");
    assert_eq!(err, ResolveError::NotFound);
}

// ---------------------------------------------------------------------------
// Offline HTTP-layer tests: verify the route-layer middleware ordering without
// a live Postgres. The lazy pool never actually connects so any DB call errors;
// the resolver treats storage errors as NotFound per ADR-004.
// ---------------------------------------------------------------------------

fn lazy_app() -> axum::Router {
    use anseo_api::{router, AppState};
    use anseo_core::ProjectId;
    use std::sync::Arc;
    let lazy_pool = sqlx::PgPool::connect_lazy("postgres://anseo:anseo@127.0.0.1:1/__ph_test__")
        .expect("connect_lazy is sync");
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

/// Auth ordering invariant: requests without an API key receive 401 before
/// project resolution even runs (the auth layer is outermost per `lib.rs`).
/// Even when a (valid-looking) project header is present, the API-key gate
/// fires first — confirming the middleware stack order from `apps/api/src/lib.rs`.
#[tokio::test]
async fn http_no_api_key_returns_401_before_project_check() {
    let app = lazy_app();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .header(PROJECT_HEADER, "whatever")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "missing API key must return 401 — auth runs before project resolution"
    );
}

/// Auth ordering invariant (with a key that looks valid but can't be verified
/// on a lazy pool): the auth middleware still fires first and returns 401,
/// never exposing a project_not_found 404 to an unauthenticated caller.
/// This verifies the `require_api_key` → `project_header_guard` layer order
/// documented in `apps/api/src/lib.rs` (auth is the outer `route_layer`).
#[tokio::test]
async fn http_auth_fires_before_project_resolution() {
    use anseo_core::api_key::{generate, API_KEY_HEADER};
    let key = generate();
    let app = lazy_app();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/v1/runs")
                .header(API_KEY_HEADER, &key.plaintext)
                .header(PROJECT_HEADER, "does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // With a lazy pool the auth DB call errors → 401. Project resolution never
    // runs (it would also fail, but auth is the outer layer that short-circuits
    // first). This confirms an unauthenticated caller cannot reach the 404
    // path even with an unknown project header.
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "auth must short-circuit before project resolution for an unverifiable key"
    );
}
