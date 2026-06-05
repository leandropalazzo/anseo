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
//! - **CLI** resolves the project via the ADR-004 precedence chain
//!   ([`opengeo_cli::commands::project::resolve_project_id`]) directly against
//!   the `projects` table — no HTTP hop. The marker file or `--project` flag
//!   acts as the ambient tier (equivalent to the `X-OpenGEO-Project` header on
//!   the other surfaces).
//!
//! This test stands up the REAL `opengeo-api` router on an ephemeral port
//! against a live Postgres, seeds two projects A and B, then drives a
//! project-scoped read (`GET /v1/setup/brand`, which echoes the resolved
//! project's `project_id` + `name`) over ALL THREE surfaces:
//!
//!   1. raw HTTP with `X-OpenGEO-Project` (the Web-proxy shape);
//!   2. the MCP [`ApiClient`] with the same project pinned; and
//!   3. the CLI `resolve_project_id` resolver driven in-process (the CLI shape).
//!
//! and asserts:
//!   - selecting A yields A-scoped data on all three surfaces, identically;
//!   - selecting B yields B-scoped data on all three surfaces, identically;
//!   - switching A→B flips the result the same way on every surface;
//!   - CLI resolution via `--project` flag matches the header and MCP selector;
//!   - CLI resolution via the `.opengeo/selected-project` marker also matches;
//!   - an unknown project is rejected consistently on all three surfaces.
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
use opengeo_cli::commands::project as cli_project;
use opengeo_core::api_key::generate as gen_key;
use opengeo_core::{BrandConfig, ProjectId};
use opengeo_mcp::http_client::ApiClient;
use opengeo_storage::repositories::{api_keys::ApiKeyRepo, projects::ProjectRepo};
use opengeo_storage::Storage;
use sqlx::PgPool;
use tempfile::TempDir;

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
    /// Storage handle for CLI resolver calls (direct-DB, no HTTP hop).
    storage: Storage,
}

/// Boot the real API router on an ephemeral loopback port against a live DB,
/// seed projects A + B and an API key. Returns `None` when `DATABASE_URL` is
/// unset so the suite is a no-op in offline runs (matching sibling tests).
async fn boot() -> Option<Harness> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage_arc = Arc::new(Storage::from_pool(pool.clone()));
    storage_arc.migrate().await.expect("migrate");

    // A plain (non-Arc) Storage handle for CLI resolver calls.
    let storage_cli = Storage::from_pool(pool.clone());

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
        storage: storage_arc,
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
        storage: storage_cli,
    })
}

// ── Surface helpers ─────────────────────────────────────────────────────────

/// Surface 1 — raw HTTP with `X-OpenGEO-Project` (the Web-proxy shape).
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

/// Surface 3a — CLI resolver with explicit `--project <name>` flag.
///
/// This exercises the highest-precedence tier of the ADR-004 CLI chain: the
/// `--project` flag out-ranks every ambient selection (marker, YAML, sole
/// fallback). The resolver returns the same `ProjectId` the header surfaces
/// resolve to, confirming there is no skew between the three surfaces.
async fn via_cli_flag(h: &Harness, project: &str) -> ProjectId {
    let dir = TempDir::new().expect("tempdir");
    cli_project::resolve_project_id(&h.storage, dir.path(), Some(project))
        .await
        .unwrap_or_else(|e| panic!("CLI flag resolve failed for '{project}': {e}"))
}

/// Surface 3b — CLI resolver with the `.opengeo/selected-project` marker.
///
/// This exercises the ambient working-dir tier of ADR-004: `ogeo project use`
/// writes the marker, and subsequent commands pick it up without a flag.
/// We write the marker directly (same byte format as `run_use`) to drive the
/// resolver without spawning the binary.
async fn via_cli_marker(h: &Harness, project_id: ProjectId) -> ProjectId {
    let dir = TempDir::new().expect("tempdir");
    // Write the marker in the same format `run_use` does.
    let marker_dir = dir.path().join(".opengeo");
    std::fs::create_dir_all(&marker_dir).expect("create marker dir");
    std::fs::write(marker_dir.join("selected-project"), format!("{project_id}\n"))
        .expect("write marker");

    cli_project::resolve_project_id(&h.storage, dir.path(), None)
        .await
        .unwrap_or_else(|e| panic!("CLI marker resolve failed for '{project_id}': {e}"))
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// AC1 + AC2: project P and project Q both return consistently scoped data on
/// every surface, and switching P→Q produces the identical flip on all three.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn project_selection_is_consistent_across_all_three_surfaces() {
    let Some(h) = boot().await else {
        return;
    };

    // ── Select project A ────────────────────────────────────────────────────
    let a_header = via_header(&h, &h.project_a).await;
    let a_mcp = via_mcp(&h, &h.project_a).await;
    let a_cli_flag = via_cli_flag(&h, &h.project_a).await;
    let a_cli_marker = via_cli_marker(&h, h.id_a).await;

    // The HTTP/MCP scoped read echoes the RESOLVED project's identity.
    assert_eq!(a_header["project_id"], h.id_a.to_string());
    assert_eq!(a_header["name"], h.project_a);

    // Web-proxy path == MCP selector path, byte-for-byte.
    assert_eq!(
        a_header, a_mcp,
        "selecting A: header and MCP surfaces must return identical data"
    );

    // CLI (flag) resolves to the same project_id as the header surface.
    assert_eq!(
        a_cli_flag,
        h.id_a,
        "selecting A via --project flag: CLI must resolve to A's id"
    );
    // CLI (marker) resolves to the same project_id.
    assert_eq!(
        a_cli_marker,
        h.id_a,
        "selecting A via marker: CLI must resolve to A's id"
    );

    // ── Switch to project B ─────────────────────────────────────────────────
    let b_header = via_header(&h, &h.project_b).await;
    let b_mcp = via_mcp(&h, &h.project_b).await;
    let b_cli_flag = via_cli_flag(&h, &h.project_b).await;
    let b_cli_marker = via_cli_marker(&h, h.id_b).await;

    assert_eq!(b_header["project_id"], h.id_b.to_string());
    assert_eq!(b_header["name"], h.project_b);
    assert_eq!(
        b_header, b_mcp,
        "selecting B: header and MCP surfaces must return identical data"
    );
    assert_eq!(
        b_cli_flag,
        h.id_b,
        "selecting B via --project flag: CLI must resolve to B's id"
    );
    assert_eq!(
        b_cli_marker,
        h.id_b,
        "selecting B via marker: CLI must resolve to B's id"
    );

    // ── Switching A→B produces identical flips on every surface ────────────
    // The two projects must be distinguishable (no silent cross-project bleed).
    assert_ne!(
        a_header, b_header,
        "header surface: A and B must be distinguishable"
    );
    assert_ne!(a_mcp, b_mcp, "MCP surface: A and B must be distinguishable");
    assert_ne!(a_cli_flag, b_cli_flag, "CLI flag: A and B must differ");
    assert_ne!(a_cli_marker, b_cli_marker, "CLI marker: A and B must differ");

    // The delta observed on the HTTP and MCP surfaces equals the delta the CLI
    // resolver observed: every surface moved from exactly A to exactly B.
    assert_eq!(a_header["project_id"].as_str().unwrap(), a_cli_flag.to_string().as_str(),
        "A: header project_id == CLI flag project_id");
    assert_eq!(b_header["project_id"].as_str().unwrap(), b_cli_flag.to_string().as_str(),
        "B: header project_id == CLI flag project_id");
    assert_eq!(a_cli_flag, a_cli_marker,
        "CLI flag and marker must resolve to the same id for A");
    assert_eq!(b_cli_flag, b_cli_marker,
        "CLI flag and marker must resolve to the same id for B");
}

/// An unknown project is rejected identically on all three surfaces — a wrong
/// selector never silently falls through to another project's data.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn unknown_project_is_rejected_on_all_three_surfaces() {
    let Some(h) = boot().await else {
        return;
    };
    let bogus = format!("no-such-project-{}", uuid::Uuid::new_v4());

    // Surface 1 — raw header (Web shape): unknown project → 404.
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

    // Surface 2 — MCP selector: same rejection.
    let api = ApiClient::new(h.base_url.clone(), h.key.clone(), bogus.clone())
        .expect("ApiClient");
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

    // Surface 3 — CLI resolver: an unknown name must return a config error, not
    // silently fall through to another project. (With two projects in the DB the
    // legacy sole-active fallback is disabled, so a bad name must always error.)
    let dir = TempDir::new().expect("tempdir");
    let cli_result =
        cli_project::resolve_project_id(&h.storage, dir.path(), Some(&bogus)).await;
    assert!(
        cli_result.is_err(),
        "CLI --project with an unknown name must return Err (got Ok({:?}))",
        cli_result.ok()
    );
}

/// Re-selecting the SAME project is idempotent on every surface.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires DATABASE_URL"]
async fn reselecting_same_project_is_stable_on_all_surfaces() {
    let Some(h) = boot().await else {
        return;
    };

    // Header surface: two consecutive reads of A return the same body.
    let a1 = via_header(&h, &h.project_a).await;
    let a2 = via_header(&h, &h.project_a).await;
    assert_eq!(a1, a2, "header surface: re-selecting A must be stable");

    // MCP surface: two consecutive reads of A.
    let m1 = via_mcp(&h, &h.project_a).await;
    let m2 = via_mcp(&h, &h.project_a).await;
    assert_eq!(m1, m2, "MCP surface: re-selecting A must be stable");

    // CLI (flag): two consecutive resolutions of A.
    let c1 = via_cli_flag(&h, &h.project_a).await;
    let c2 = via_cli_flag(&h, &h.project_a).await;
    assert_eq!(c1, c2, "CLI flag: re-selecting A must be stable");

    // All three agree on the final resolved id.
    assert_eq!(a1["project_id"].as_str().unwrap(), c1.to_string().as_str(),
        "header and CLI flag must agree on A's project_id");
    assert_eq!(m1["project_id"].as_str().unwrap(), c1.to_string().as_str(),
        "MCP and CLI flag must agree on A's project_id");
}
