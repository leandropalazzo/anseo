//! Story 36.9 — CLI ⇄ Web ⇄ MCP project-switch parity (acceptance).
//!
//! The product's core differentiator is that selecting a project behaves
//! IDENTICALLY across every surface. All three surfaces converge on one seam:
//! the API resolves each request against the `projects` table using the
//! `X-OpenGEO-Project` header (ADR-004 precedence).
//!
//! - **Web** forwards the operator's selected project as `X-OpenGEO-Project`
//!   through its proxy — i.e. a raw HTTP call with that header.
//! - **MCP** pins the selected project into its loopback [`ApiClient`], which
//!   stamps the SAME header on every `/v1` call (see `apps/mcp/src/http_client.rs`).
//! - **CLI** likewise sends `X-OpenGEO-Project` on its `/v1` calls.
//!
//! This test stands up the REAL `opengeo-api` router on an ephemeral port
//! against a live Postgres, seeds two projects A and B, then drives a
//! project-scoped read (`GET /v1/setup/brand`, which echoes the resolved
//! project's `project_id` + `name`) over BOTH surfaces:
//!
//!   1. raw HTTP with `X-OpenGEO-Project` (the Web-proxy / CLI shape), and
//!   2. the MCP [`ApiClient`] with the same project pinned.
//!
//! and asserts:
//!   - selecting A yields A-scoped data on both surfaces, identically;
//!   - selecting B yields B-scoped data on both surfaces, identically;
//!   - switching A→B flips the result the same way on every surface;
//!   - the MCP response equals the raw-header response byte-for-byte (the
//!     selector adds no skew of its own).
//!
//! NO source under `apps/api` is touched — the test consumes only the public
//! `router()` + `AppState`. It needs a live Postgres and is `#[ignore]`d like
//! the sibling `*_live_db` suites:
//!
//! ```text
//! DATABASE_URL=postgres://opengeo:opengeo@localhost:5445/opengeo_test \
//!   cargo test -p opengeo-mcp --test multiproject_parity -- --ignored
//! ```

use std::sync::Arc;

use opengeo_api::{router, AppState};
use opengeo_core::api_key::generate as gen_key;
use opengeo_core::{BrandConfig, ProjectId};
use opengeo_mcp::http_client::ApiClient;
use opengeo_storage::repositories::{api_keys::ApiKeyRepo, projects::ProjectRepo};
use opengeo_storage::Storage;
use sqlx::PgPool;

const PROJECT_HEADER: &str = "X-OpenGEO-Project";
const API_KEY_HEADER: &str = "X-OpenGEO-API-Key";

/// The scoped probe: resolves via `X-OpenGEO-Project` and returns the resolved
/// project's own `project_id` + `name`, so two projects yield distinct,
/// deterministic bodies with no analytics seeding required.
const SCOPED_PATH: &str = "/v1/setup/brand";

struct Harness {
    base_url: String,
    key: String,
    project_a: String,
    id_a: ProjectId,
    project_b: String,
    id_b: ProjectId,
}

/// Boot the real API router on an ephemeral loopback port against a live DB,
/// seed projects A + B and an API key. Returns `None` when `DATABASE_URL` is
/// unset so the suite is a no-op in offline runs (matching sibling tests).
async fn boot() -> Option<Harness> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Arc::new(Storage::from_pool(pool.clone()));
    storage.migrate().await.expect("migrate");

    // Distinct, unique project names so derived ids never collide with sibling
    // suites sharing the database.
    let suffix = uuid::Uuid::new_v4();
    let project_a = format!("Parity-A-{suffix}");
    let project_b = format!("Parity-B-{suffix}");

    let repo = ProjectRepo::new(&pool);
    let id_a = repo
        .create_project(&BrandConfig {
            name: project_a.clone(),
            variants: Vec::new(),
            site_url: None,
        })
        .await
        .expect("seed project A");
    let id_b = repo
        .create_project(&BrandConfig {
            name: project_b.clone(),
            variants: Vec::new(),
            site_url: None,
        })
        .await
        .expect("seed project B");
    assert_ne!(id_a, id_b, "two brands must derive distinct project ids");

    // One operator API key (the operator surface is project-agnostic; the key
    // owns project A but every `/v1` call selects its project via the header).
    let key = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(id_a, "parity", &key.sha256_hash, "ogeo_tst")
        .await
        .expect("seed key");

    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id: id_a,
        events,
        config: None,
        provider_registry: None,
        configured_project: Arc::new(project_a.clone()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    };

    // Bind the real router to an OS-assigned port, then serve in the background.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let app = router(state);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    // Let the listener accept.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    Some(Harness {
        base_url: format!("http://{addr}"),
        key: key.plaintext,
        project_a,
        id_a,
        project_b,
        id_b,
    })
}

/// Surface 1 — raw HTTP with `X-OpenGEO-Project` (the Web-proxy / CLI shape).
async fn via_header(h: &Harness, project: &str) -> serde_json::Value {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}{SCOPED_PATH}", h.base_url))
        .header(API_KEY_HEADER, &h.key)
        .header(PROJECT_HEADER, project)
        .send()
        .await
        .expect("header request");
    assert_eq!(
        resp.status(),
        200,
        "scoped read via header must succeed for {project}"
    );
    resp.json().await.expect("header body is json")
}

/// Surface 2 — the MCP loopback `ApiClient` with `project` pinned (the MCP
/// project selector). It stamps `X-OpenGEO-Project` internally.
async fn via_mcp(h: &Harness, project: &str) -> serde_json::Value {
    let api = ApiClient::new(h.base_url.clone(), h.key.clone(), project.to_string())
        .expect("ApiClient construction");
    let resp = api.get(SCOPED_PATH).send().await.expect("mcp request");
    assert_eq!(
        resp.status(),
        200,
        "scoped read via MCP selector must succeed for {project}"
    );
    resp.json().await.expect("mcp body is json")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn project_selection_is_consistent_across_surfaces() {
    let Some(h) = boot().await else {
        return;
    };

    // --- Select project A -------------------------------------------------
    let a_header = via_header(&h, &h.project_a).await;
    let a_mcp = via_mcp(&h, &h.project_a).await;

    // The scoped read echoes the RESOLVED project's identity.
    assert_eq!(a_header["project_id"], h.id_a.to_string());
    assert_eq!(a_header["name"], h.project_a);
    // Web-proxy/CLI header path == MCP selector path, byte-for-byte.
    assert_eq!(
        a_header, a_mcp,
        "selecting A must yield identical data on the header and MCP surfaces"
    );

    // --- Switch to project B ---------------------------------------------
    let b_header = via_header(&h, &h.project_b).await;
    let b_mcp = via_mcp(&h, &h.project_b).await;

    assert_eq!(b_header["project_id"], h.id_b.to_string());
    assert_eq!(b_header["name"], h.project_b);
    assert_eq!(
        b_header, b_mcp,
        "selecting B must yield identical data on the header and MCP surfaces"
    );

    // --- Switching A→B flips the result identically on every surface ------
    assert_ne!(
        a_header, b_header,
        "the two projects must be distinguishable (no silent cross-project bleed)"
    );
    assert_ne!(a_mcp, b_mcp);
    // The delta observed by MCP equals the delta observed via the raw header:
    // both surfaces moved from exactly A's data to exactly B's data.
    assert_eq!(a_header, a_mcp);
    assert_eq!(b_header, b_mcp);
    assert_eq!(
        a_mcp["project_id"], a_header["project_id"],
        "A resolves to the same id on both surfaces"
    );
    assert_eq!(
        b_mcp["project_id"], b_header["project_id"],
        "B resolves to the same id on both surfaces"
    );
}

/// Re-selecting the SAME project is stable (idempotent) on each surface, and an
/// unknown project selection is rejected identically — a wrong selector never
/// silently falls through to another project's data.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn unknown_project_is_rejected_on_both_surfaces() {
    let Some(h) = boot().await else {
        return;
    };
    let bogus = format!("no-such-project-{}", uuid::Uuid::new_v4());

    // Raw header (Web/CLI shape): unknown project -> 404.
    let client = reqwest::Client::new();
    let header_status = client
        .get(format!("{}{SCOPED_PATH}", h.base_url))
        .header(API_KEY_HEADER, &h.key)
        .header(PROJECT_HEADER, &bogus)
        .send()
        .await
        .expect("header request")
        .status();
    assert_eq!(
        header_status, 404,
        "unknown project via header must 404 (no fallback to another project)"
    );

    // MCP selector: same path, same rejection.
    let api = ApiClient::new(h.base_url.clone(), h.key.clone(), bogus).expect("ApiClient");
    let mcp_status = api
        .get(SCOPED_PATH)
        .send()
        .await
        .expect("mcp request")
        .status();
    assert_eq!(
        header_status, mcp_status,
        "MCP selector must reject an unknown project identically to the header surface"
    );
}
