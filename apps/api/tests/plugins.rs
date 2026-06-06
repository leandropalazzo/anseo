//! Story 41.2 — `GET /v1/plugins` router-level coverage + plugin load-path
//! integration (AC1/AC4/AC6).
//!
//! The route-shape tests use a lazy Postgres pool that never IOs: the auth
//! middleware short-circuits with 401 before the handler runs, so we can assert
//! the endpoint is wired + gated without a live DB. The load-path integration
//! test (AC1/AC6) exercises the real loader against a fixture dropped directly
//! into a temp plugin home — no network, no `ogeo plugin install` (which lands
//! in 41.3); per AC1's note, the load-path is tested independently.

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::ProjectId;
use anseo_plugin_host::loader::{scan_and_load, LoadPolicy, LoadStatus};
use anseo_plugin_host::subprocess::Platform;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn build_router(loaded: Vec<anseo_plugin_host::loader::LoadedPlugin>) -> axum::Router {
    let lazy_pool =
        sqlx::PgPool::connect_lazy("postgres://opengeo:opengeo@127.0.0.1:1/__plugins_test__")
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
        loaded_plugins: Arc::new(loaded),
    };
    router(state)
}

#[tokio::test]
async fn plugins_endpoint_requires_auth() {
    // `/v1/plugins` lives on the operator surface (require_api_key, no project
    // guard): no key ⇒ 401, before any DB IO.
    let app = build_router(Vec::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/plugins")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// AC1 + AC6: drop a valid plugin fixture into a plugin home, scan it (the same
/// pass `anseo serve` runs at boot), and assert it reports `status: loaded` —
/// i.e. the load-path activates an installed plugin without manual
/// re-registration. The `AppState.loaded_plugins` field this populates is what
/// `GET /v1/plugins` serves verbatim.
#[test]
fn load_path_reports_loaded_for_fixture_plugin() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let dir = home.join("plugins").join("acme/provider").join("0.1.0");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        "name: acme/provider\nversion: \"0.1.0\"\ncapabilities: []\nplugin_type: provider\nentry_point: entrypoint.wasm\n",
    )
    .unwrap();
    std::fs::write(
        home.join("installed.toml"),
        "[[plugin]]\nid = \"acme/provider\"\nversion = \"0.1.0\"\nsignature_status = \"signed\"\n",
    )
    .unwrap();

    let policy = LoadPolicy {
        allow_unsigned: false,
        platform: Platform::Linux,
    };
    let report = scan_and_load(home, &policy);
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].id, "acme/provider");
    assert_eq!(report[0].kind, "provider");
    assert_eq!(report[0].status, LoadStatus::Loaded);
}

/// The load report flows into a router build (the `AppState.loaded_plugins`
/// field `GET /v1/plugins` serializes) without panicking. Async so the lazy
/// pool in `build_router` has a Tokio context.
#[tokio::test]
async fn loaded_report_builds_router() {
    let report = vec![anseo_plugin_host::loader::LoadedPlugin {
        id: "acme/provider".to_string(),
        version: "0.1.0".to_string(),
        kind: "provider".to_string(),
        status: LoadStatus::Loaded,
        reason: String::new(),
    }];
    let _app = build_router(report);
}
