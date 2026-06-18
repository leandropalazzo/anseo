//! Story 49.0 — Plane-1 OSS operator substrate, live-Postgres coverage.
//!
//! Gated `#[ignore]` so default offline `cargo test` stays green; CI runs these
//! with a live DB. Run via:
//!
//! ```text
//! ANSEO_OPERATOR_API_KEY=ogeo_operator_test_key_value_0000 \
//! DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo \
//!   cargo test -p anseo-api --test operator_plane1_live_db -- --ignored
//! ```
//!
//! Covers: 401/403 authz on every route; read-only consent records/events
//! (filter + paginate); kek-status never exposes key material; gate read/write
//! round-trip (PUT changes the OSS source of truth, GET reflects it); the gate
//! is readable from the OSS storage layer WITHOUT touching anseo_admin; density
//! parity (meets_floor uses the gate's floor with the same `>=` predicate).

use std::sync::Arc;

use anseo_api::{router, AppState};
use anseo_core::api_key::generate as gen_key;
use anseo_core::ProjectId;
use anseo_storage::models::ProjectRow;
use anseo_storage::repositories::api_keys::ApiKeyRepo;
use anseo_storage::repositories::projects::ProjectRepo;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use sqlx::PgPool;
use tower::ServiceExt;

const OPERATOR_KEY: &str = "ogeo_operator_test_key_value_0000";
const OPERATOR_HEADER: &str = "X-Anseo-API-Key";

async fn setup() -> (axum::Router, String, PgPool, ProjectId) {
    std::env::set_var("ANSEO_OPERATOR_API_KEY", OPERATOR_KEY);
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for operator_plane1_live_db");
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));
    storage.migrate().await.expect("migrate");

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
    let tenant = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(
            project_id,
            "tenant-key",
            &tenant.sha256_hash,
            &tenant.display_prefix,
        )
        .await
        .expect("seed tenant key");

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
            rate_limit: anseo_api::middleware::rate_limit::RateLimitStore::new(),
    };
    (router(state), tenant.plaintext, pool, project_id)
}

/// Seed a consent event row directly (OSS-owned `benchmark_consent`).
async fn seed_consent(
    pool: &PgPool,
    project_id: ProjectId,
    event: &str,
    tier: &str,
    terms_version: &str,
) -> uuid::Uuid {
    let id = uuid::Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO benchmark_consent (id, project_id, event, terms_version, tier)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(id)
    .bind(project_id)
    .bind(event)
    .bind(terms_version)
    .bind(tier)
    .execute(pool)
    .await
    .expect("seed consent");
    id
}

async fn req(
    app: &axum::Router,
    method: &str,
    uri: &str,
    key: Option<&str>,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(k) = key {
        b = b.header(OPERATOR_HEADER, k);
    }
    let body = match body {
        Some(j) => {
            b = b.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&j).unwrap())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 256 * 1024)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

const READ_ROUTES: &[(&str, &str)] = &[
    ("GET", "/v1/operator/consent/records"),
    ("GET", "/v1/operator/consent/events"),
    ("GET", "/v1/operator/consent/kek-status"),
    ("GET", "/v1/operator/contributions/density"),
    ("GET", "/v1/operator/verification/throughput"),
    ("GET", "/v1/operator/config/benchmark-gate"),
];

// ─── auth gate (every route): 401 no key / 403 tenant key ────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn no_key_and_tenant_key_are_rejected_on_every_route() {
    let (app, tenant_key, _pool, _p) = setup().await;
    for (m, uri) in READ_ROUTES {
        let (s, _) = req(&app, m, uri, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "no-key {m} {uri}");
        let (s, _) = req(&app, m, uri, Some(&tenant_key), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "tenant-key {m} {uri}");
    }
    // PUT gate too.
    let put = "/v1/operator/config/benchmark-gate";
    let (s, _) = req(&app, "PUT", put, None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no-key PUT {put}");
    let (s, _) = req(&app, "PUT", put, Some(&tenant_key), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "tenant-key PUT {put}");
}

// ─── consent records: filter + paginate, read-only ──────────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn consent_records_filter_and_paginate() {
    let (app, _t, pool, project_id) = setup().await;
    seed_consent(&pool, project_id, "optin", "brand_visibility", "v1").await;
    seed_consent(&pool, project_id, "optout", "brand_visibility", "v1").await;
    seed_consent(&pool, project_id, "optin", "anonymous", "v1").await;

    // Filter by this project + tier brand_visibility → exactly the 2 bv rows.
    let uri = format!("/v1/operator/consent/records?project={project_id}&tier=brand_visibility");
    let (s, body) = req(&app, "GET", &uri, Some(OPERATOR_KEY), None).await;
    assert_eq!(s, StatusCode::OK);
    let recs = body["records"].as_array().unwrap();
    assert_eq!(recs.len(), 2, "tier+project filter: {body}");
    assert!(recs.iter().all(|r| r["tier"] == "brand_visibility"));

    // event=optin AND project → exactly 1 bv optin + 1 anon optin = 2.
    let uri = format!("/v1/operator/consent/records?project={project_id}&event=optin");
    let (_, body) = req(&app, "GET", &uri, Some(OPERATOR_KEY), None).await;
    assert_eq!(body["records"].as_array().unwrap().len(), 2);

    // Pagination: limit=1 on the bv-tier set returns 1.
    let uri =
        format!("/v1/operator/consent/records?project={project_id}&tier=brand_visibility&limit=1");
    let (_, body) = req(&app, "GET", &uri, Some(OPERATOR_KEY), None).await;
    assert_eq!(body["records"].as_array().unwrap().len(), 1);

    // Bad tier → 400.
    let (s, _) = req(
        &app,
        "GET",
        "/v1/operator/consent/records?tier=bogus",
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn consent_events_carry_terms_version_and_timestamp() {
    let (app, _t, pool, project_id) = setup().await;
    seed_consent(&pool, project_id, "optin", "brand_visibility", "vX").await;
    let uri = format!("/v1/operator/consent/events?project={project_id}");
    let (s, body) = req(&app, "GET", &uri, Some(OPERATOR_KEY), None).await;
    assert_eq!(s, StatusCode::OK);
    let ev = &body["events"][0];
    assert_eq!(ev["event"], "optin");
    assert_eq!(ev["terms_version"], "vX");
    assert!(ev["created_at"].is_string());
}

// ─── kek-status: never exposes key material ──────────────────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn kek_status_never_exposes_key_material() {
    let (app, _t, pool, project_id) = setup().await;
    seed_consent(&pool, project_id, "optin", "brand_visibility", "v1").await;
    let (s, body) = req(
        &app,
        "GET",
        "/v1/operator/consent/kek-status",
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    // The serialized payload must carry only project_id + a status string —
    // never any key/secret material. Assert the per-project shape and that no
    // key-bearing field names appear anywhere in the response text.
    let raw = body.to_string();
    for forbidden in [
        "key",
        "secret",
        "kek_value",
        "material",
        "dek",
        "passphrase",
    ] {
        assert!(
            !raw.to_lowercase().contains(forbidden),
            "kek-status leaked '{forbidden}': {raw}"
        );
    }
    let proj = body["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["project_id"] == project_id.to_string())
        .expect("our project present");
    // No KEK provisioned + no identified contributions → pending.
    assert_eq!(proj["status"], "pending");
    // Exactly two fields per project entry.
    assert_eq!(
        proj.as_object().unwrap().len(),
        2,
        "only project_id + status"
    );
}

// ─── gate read/write round-trip: PUT changes OSS source of truth ─────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn gate_put_changes_source_of_truth_and_get_reflects_it() {
    let (app, _t, _pool, _p) = setup().await;

    // PUT finalizes terms at a new version + floor.
    let put_body = serde_json::json!({
        "terms_finalized": true,
        "terms_version": "2026-06-roundtrip",
        "density_floor": 7,
        "operator": "alice"
    });
    let (s, body) = req(
        &app,
        "PUT",
        "/v1/operator/config/benchmark-gate",
        Some(OPERATOR_KEY),
        Some(put_body),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["terms_finalized"], true);
    assert_eq!(body["terms_version"], "2026-06-roundtrip");
    assert_eq!(body["density_floor"], 7);

    // A subsequent GET reflects the persisted write.
    let (s, body) = req(
        &app,
        "GET",
        "/v1/operator/config/benchmark-gate",
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["terms_finalized"], true);
    assert_eq!(body["terms_version"], "2026-06-roundtrip");
    assert_eq!(body["density_floor"], 7);

    // Empty terms_version → 400.
    let (s, _) = req(
        &app,
        "PUT",
        "/v1/operator/config/benchmark-gate",
        Some(OPERATOR_KEY),
        Some(serde_json::json!({
            "terms_finalized": true, "terms_version": "  ", "density_floor": 5
        })),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// ─── gate readable from OSS storage WITHOUT anseo_admin ──────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn gate_is_readable_by_oss_consumer_without_anseo_admin() {
    // Simulates the CLI optin / ingest path: it reads the gate via the OSS
    // storage layer (crates/storage), which queries the OSS `benchmark_gate_config`
    // table only — there is no `anseo_admin` schema in the OSS database at all.
    let (_app, _t, pool, _p) = setup().await;
    let storage = anseo_storage::Storage::from_pool(pool.clone());

    // Write via the OSS repo (the operator endpoint's source of truth path).
    storage
        .benchmark_gate()
        .upsert(true, "oss-readable-v2", 5, Some("console"))
        .await
        .expect("upsert gate");

    // Read it back with NO reference to anseo_admin.
    let gate = storage.benchmark_gate().get().await.expect("read gate");
    assert!(gate.terms_finalized);
    assert_eq!(gate.terms_version, "oss-readable-v2");

    // Prove no anseo_admin schema exists in the OSS DB (so a read here could not
    // possibly be reading it).
    let admin_schema_exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = 'anseo_admin')"#,
    )
    .fetch_one(&pool)
    .await
    .expect("schema check");
    assert!(
        !admin_schema_exists,
        "OSS database must not contain an anseo_admin schema (ADR-007)"
    );
}

// ─── density parity: meets_floor uses the gate floor + the same predicate ────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn density_floor_parity_with_gate_and_floor_predicate() {
    let (app, _t, _pool, _p) = setup().await;

    // Reads succeed even when the externally-populated benchmark_segment_stats
    // table is absent (tolerated → empty segments), matching density_check's
    // unwrap_or posture.
    let (s, body) = req(
        &app,
        "GET",
        "/v1/operator/contributions/density",
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert!(body["segments"].is_array());
    // The reported floor is the OSS-owned gate floor (default 5 here), which is
    // the SAME k-anonymity floor (`contributor_count >= 5`) the public-benchmark
    // density-floor source of truth (density_check) applies.
    assert_eq!(body["density_floor"], 5);
    assert_eq!(body["window_days"], 30);
}

// ─── a non-default gate floor moves BOTH surfaces' verdict consistently ───────
//
// 49.0 follow-up: density_check (public benchmark) used to HARDCODE the literal
// `contributor_count >= 5`, so it could disagree with the operator density
// endpoint when the gate floor != 5. Both now read the gate floor. This test
// seeds a category that sits BETWEEN the default floor (5) and a raised floor
// (7), then proves raising the gate floor flips the verdict the SAME way on
// BOTH surfaces:
//   - operator endpoint  → segment.meets_floor
//   - public density_check → categories_above_k5_floor.actual count
async fn seed_segment(pool: &PgPool, provider: &str, category: &str, count: i64) {
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS benchmark_segment_stats (
               provider          text   NOT NULL,
               category          text   NOT NULL,
               window_days       bigint NOT NULL,
               contributor_count bigint NOT NULL
           )"#,
    )
    .execute(pool)
    .await
    .expect("create benchmark_segment_stats");
    sqlx::query(
        r#"INSERT INTO benchmark_segment_stats (provider, category, window_days, contributor_count)
           VALUES ($1, $2, 30, $3)"#,
    )
    .bind(provider)
    .bind(category)
    .bind(count)
    .execute(pool)
    .await
    .expect("seed segment");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn nondefault_gate_floor_moves_both_surfaces_consistently() {
    let (app, _t, pool, _p) = setup().await;

    // A category with exactly 6 contributors: ABOVE the default floor (5) but
    // BELOW a raised floor (7). Its verdict therefore hinges on the floor.
    seed_segment(&pool, "openai", "fintech", 6).await;

    // ── default floor (5): the 6-contributor segment counts as above-floor ──
    let (s, dens) = req(&app, "GET", "/v1/benchmark/density-check", None, None).await;
    assert_eq!(s, StatusCode::OK, "{dens}");
    let cat_detail = dens["thresholds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["threshold_name"] == "categories_above_k5_floor")
        .expect("categories_above_k5_floor threshold present");
    assert_eq!(
        cat_detail["actual"], 1,
        "at floor 5 the 6-contributor category is above-floor"
    );

    let (_s, op) = req(
        &app,
        "GET",
        "/v1/operator/contributions/density",
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(op["density_floor"], 5);
    let seg = op["segments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["category"] == "fintech")
        .expect("fintech segment present");
    assert_eq!(
        seg["meets_floor"], true,
        "operator endpoint agrees: above floor 5"
    );

    // ── raise the gate floor to 7 (the single source of truth) ──
    let storage = anseo_storage::Storage::from_pool(pool.clone());
    storage
        .benchmark_gate()
        .upsert(true, "floor-7", 7, Some("console"))
        .await
        .expect("raise floor");

    // density_check now reads floor 7 → the 6-contributor category drops below.
    let (_s, dens) = req(&app, "GET", "/v1/benchmark/density-check", None, None).await;
    let cat_detail = dens["thresholds"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["threshold_name"] == "categories_above_k5_floor")
        .unwrap();
    assert_eq!(
        cat_detail["actual"], 0,
        "at floor 7 the 6-contributor category is NO LONGER above-floor"
    );

    // Operator endpoint flips the SAME way — the two surfaces stay consistent.
    let (_s, op) = req(
        &app,
        "GET",
        "/v1/operator/contributions/density",
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(op["density_floor"], 7);
    let seg = op["segments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["category"] == "fintech")
        .unwrap();
    assert_eq!(
        seg["meets_floor"], false,
        "operator endpoint flips consistently: below floor 7"
    );

    // Clean up the externally-shaped table so it can't leak into other tests.
    sqlx::query("DROP TABLE IF EXISTS benchmark_segment_stats")
        .execute(&pool)
        .await
        .ok();
}
