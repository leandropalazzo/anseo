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
        sqlx::PgPool::connect_lazy("postgres://anseo:anseo@127.0.0.1:1/__plugins_test__")
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
            rate_limit: anseo_api::middleware::rate_limit::RateLimitStore::new(),
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

// ---------------------------------------------------------------------------
// Story 41.3 — slash-containing plugin ids round-trip through the route layer.
//
// Plugin ids are `namespace/name` (e.g. `acme/widget`). The remove/upgrade
// routes are single-segment (`/v1/plugins/:id`, `/v1/plugins/:id/upgrade`), so
// a client MUST percent-encode the slash as `%2F` for the request to match the
// route — and the server's `Path<String>` extractor decodes it back. The
// generated SDK clients (packages/python, packages/typescript) now emit that
// encoding (`quote(str(id), safe="")` / `encodeURIComponent(String(id))`).
// These tests pin the server half of that contract so the round-trip can't
// regress silently.
// ---------------------------------------------------------------------------

/// The percent-encoded form matches the route (handler reached → auth layer
/// returns 401, *not* the router's 404), while the raw-slash form a buggy
/// client would send does NOT match. The 401-vs-404 contrast is the whole
/// reason the SDK must encode.
#[tokio::test]
async fn slash_plugin_id_routes_match_only_when_percent_encoded() {
    let app = build_router(Vec::new());

    // DELETE /v1/plugins/acme%2Fwidget — encoded slash ⇒ single segment ⇒
    // matches `/v1/plugins/:id`; auth rejects before the handler runs.
    let encoded_remove = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/plugins/acme%2Fwidget")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        encoded_remove.status(),
        StatusCode::UNAUTHORIZED,
        "percent-encoded id must match the remove route (reach the auth layer)"
    );

    // POST /v1/plugins/acme%2Fwidget/upgrade — encoded slash keeps `id` a
    // single segment, leaving `/upgrade` as the trailing literal.
    let encoded_upgrade = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/plugins/acme%2Fwidget/upgrade")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        encoded_upgrade.status(),
        StatusCode::UNAUTHORIZED,
        "percent-encoded id must match the upgrade route (reach the auth layer)"
    );

    // DELETE /v1/plugins/acme/widget — the raw slash a non-encoding client
    // sends. `acme/widget` is two segments, so no route matches ⇒ 404. This is
    // the bug the SDK fix prevents.
    let raw_remove = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/plugins/acme/widget")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        raw_remove.status(),
        StatusCode::NOT_FOUND,
        "raw (unencoded) slash id must NOT match the single-segment route"
    );
}

/// The `Path<String>` extractor decodes `%2F` back to `/`, so the handler sees
/// the original `acme/widget` id for both the `:id` and `:id/upgrade` shapes.
/// Mirrors the exact route patterns from `routes::plugins::v1_router()` with
/// echo handlers so we observe the decoded value without auth/DB/registry.
#[tokio::test]
async fn path_extractor_decodes_percent_encoded_slash_id() {
    use axum::extract::Path;
    use axum::routing::{delete, post};

    async fn echo_id(Path(id): Path<String>) -> String {
        id
    }

    let app: axum::Router = axum::Router::new()
        .route("/v1/plugins/:id", delete(echo_id))
        .route("/v1/plugins/:id/upgrade", post(echo_id));

    for (method, uri) in [
        ("DELETE", "/v1/plugins/acme%2Fwidget"),
        ("POST", "/v1/plugins/acme%2Fwidget/upgrade"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{method} {uri}");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            &body[..],
            b"acme/widget",
            "Path extractor must decode %2F back to a slash for {uri}"
        );
    }
}
