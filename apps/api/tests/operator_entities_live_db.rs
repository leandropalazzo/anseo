//! Story 48.4 — operator entity-admin endpoints, live-Postgres coverage.
//!
//! Gated `#[ignore]` so default offline `cargo test` stays green; CI runs these
//! with a live DB. Run via:
//!
//! ```text
//! ANSEO_OPERATOR_API_KEY=ogeo_operator_test_key_value_0000 \
//! DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo \
//!   cargo test -p anseo-api --test operator_entities_live_db -- --ignored
//! ```
//!
//! Covers: list filter AND-combine + pagination; detail newest-first;
//! revoke/override transitions; override empty-reason → 400; erase two-step
//! (no token → token + nothing deleted; token → rows gone, kek_destroyed:false
//! when no mapping); 401/403 with no key / tenant key on every route.

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

async fn setup() -> (axum::Router, String, PgPool) {
    // Process-global, set to the SAME value every test so parallel runs agree.
    std::env::set_var("ANSEO_OPERATOR_API_KEY", OPERATOR_KEY);

    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported for operator_entities_live_db");
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Arc::new(anseo_storage::Storage::from_pool(pool.clone()));
    storage.migrate().await.expect("migrate");

    // A tenant project + valid tenant key, used to prove tenant keys are
    // rejected by the operator gate (403).
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
    };
    (router(state), tenant.plaintext, pool)
}

/// Seed an entity directly (the operator surface manages existing entities).
async fn seed_entity(pool: &PgPool, domain: &str, status: &str, method: Option<&str>) {
    sqlx::query(
        r#"INSERT INTO entities (domain, display_name, role, claim_status, verification_method,
                                 verified_at)
           VALUES ($1, $1, 'brand', $2, $3, CASE WHEN $2='verified' THEN now() ELSE NULL END)
           ON CONFLICT (domain) DO UPDATE SET claim_status = $2, verification_method = $3"#,
    )
    .bind(domain)
    .bind(status)
    .bind(method)
    .execute(pool)
    .await
    .expect("seed entity");
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

// ─── auth gate (every route) ────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn no_key_and_tenant_key_are_rejected_on_every_route() {
    let (app, tenant_key, _pool) = setup().await;
    let routes = [
        ("GET", "/v1/operator/entities"),
        ("GET", "/v1/operator/entities/example.com"),
        ("POST", "/v1/operator/entities/example.com/revoke"),
        ("POST", "/v1/operator/entities/example.com/override-verify"),
        ("POST", "/v1/operator/entities/example.com/retrigger"),
        ("POST", "/v1/operator/entities/example.com/erase"),
    ];
    for (m, uri) in routes {
        // No key → 401.
        let (s, _) = req(&app, m, uri, None, None).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "no-key {m} {uri}");
        // Valid TENANT key → 403 (not the operator credential).
        let (s, _) = req(&app, m, uri, Some(&tenant_key), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "tenant-key {m} {uri}");
    }
}

// ─── list: filter AND-combine + pagination ───────────────────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn list_filters_and_combine_and_paginate() {
    let (app, _t, pool) = setup().await;
    let p = format!("flt{}", uuid::Uuid::new_v4().simple());
    seed_entity(&pool, &format!("a-{p}.com"), "verified", Some("dns_txt")).await;
    seed_entity(
        &pool,
        &format!("b-{p}.com"),
        "verified",
        Some("email_magic_link"),
    )
    .await;
    seed_entity(&pool, &format!("c-{p}.com"), "revoked", Some("dns_txt")).await;

    // claim_status=verified AND verification_method=dns_txt AND domain substring
    // → only a-<p>.com.
    let uri = format!(
        "/v1/operator/entities?claim_status=verified&verification_method=dns_txt&domain={p}"
    );
    let (s, body) = req(&app, "GET", &uri, Some(OPERATOR_KEY), None).await;
    assert_eq!(s, StatusCode::OK);
    let ents = body["entities"].as_array().unwrap();
    assert_eq!(ents.len(), 1, "AND-combined filters: {body}");
    assert!(ents[0]["domain"]
        .as_str()
        .unwrap()
        .starts_with(&format!("a-{p}")));

    // Pagination: substring matches all three; limit=2 returns 2, offset=2 → 1.
    let uri = format!("/v1/operator/entities?domain={p}&limit=2&offset=0");
    let (_, body) = req(&app, "GET", &uri, Some(OPERATOR_KEY), None).await;
    assert_eq!(body["entities"].as_array().unwrap().len(), 2);
    let uri = format!("/v1/operator/entities?domain={p}&limit=2&offset=2");
    let (_, body) = req(&app, "GET", &uri, Some(OPERATOR_KEY), None).await;
    assert_eq!(body["entities"].as_array().unwrap().len(), 1);
}

// ─── detail: attempts newest-first ───────────────────────────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn detail_returns_attempts_newest_first() {
    let (app, _t, pool) = setup().await;
    let d = format!("dt{}.com", uuid::Uuid::new_v4().simple());
    seed_entity(&pool, &d, "verified", Some("dns_txt")).await;
    // Two attempts at distinct times.
    for (i, mins) in [("old", 60i64), ("new", 1)].iter() {
        sqlx::query(
            r#"INSERT INTO verification_attempts
               (id, domain, method, token_hash, status, attestation_version, expires_at, created_at)
               VALUES ($1, $2, 'dns_txt', $3, 'verified', 'v1', now(), now() - ($4 || ' minutes')::interval)"#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(&d)
        .bind(format!("hash-{i}"))
        .bind(mins.to_string())
        .execute(&pool)
        .await
        .unwrap();
    }
    let (s, body) = req(
        &app,
        "GET",
        &format!("/v1/operator/entities/{d}"),
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let atts = body["verification_attempts"].as_array().unwrap();
    assert_eq!(atts.len(), 2);
    // Newest-first: created_at[0] >= created_at[1].
    let c0 = atts[0]["created_at"].as_str().unwrap();
    let c1 = atts[1]["created_at"].as_str().unwrap();
    assert!(c0 >= c1, "attempts must be newest-first: {c0} vs {c1}");
}

// ─── revoke / override transitions ───────────────────────────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn revoke_then_detail_reflects_revoked() {
    let (app, _t, pool) = setup().await;
    let d = format!("rv{}.com", uuid::Uuid::new_v4().simple());
    seed_entity(&pool, &d, "verified", Some("dns_txt")).await;
    let (s, body) = req(
        &app,
        "POST",
        &format!("/v1/operator/entities/{d}/revoke"),
        Some(OPERATOR_KEY),
        Some(serde_json::json!({"operator":"alice"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["claim_status"], "revoked");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn override_verify_sets_manual_override_and_requires_reason() {
    let (app, _t, pool) = setup().await;
    let d = format!("ov{}.com", uuid::Uuid::new_v4().simple());
    seed_entity(&pool, &d, "pending", None).await;

    // Empty reason → 400.
    let (s, _) = req(
        &app,
        "POST",
        &format!("/v1/operator/entities/{d}/override-verify"),
        Some(OPERATOR_KEY),
        Some(serde_json::json!({"reason":"  "})),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // With reason → verified + manual_override.
    let (s, body) = req(
        &app,
        "POST",
        &format!("/v1/operator/entities/{d}/override-verify"),
        Some(OPERATOR_KEY),
        Some(serde_json::json!({"reason":"verified out of band","operator":"alice"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["claim_status"], "verified");
    assert_eq!(body["verification_method"], "manual_override");
}

// ─── erase: two-step + KEK skip when no mapping ──────────────────────────────

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn erase_two_step_token_gate_and_kek_skip() {
    let (app, _t, pool) = setup().await;
    let d = format!("er{}.com", uuid::Uuid::new_v4().simple());
    seed_entity(&pool, &d, "verified", Some("dns_txt")).await;

    // Step 1: no token → returns a confirm token and erases NOTHING.
    let (s, body) = req(
        &app,
        "POST",
        &format!("/v1/operator/entities/{d}/erase"),
        Some(OPERATOR_KEY),
        Some(serde_json::json!({"operator":"alice"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["confirm_required"], true);
    let token = body["confirm_token"].as_str().unwrap().to_string();
    // Still present.
    let (s, _) = req(
        &app,
        "GET",
        &format!("/v1/operator/entities/{d}"),
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(
        s,
        StatusCode::OK,
        "entity must still exist after token-less erase"
    );

    // Step 2: with the matching token → rows gone; no entity→project mapping
    // (no contributions seeded) → kek_destroyed:false.
    let (s, body) = req(
        &app,
        "POST",
        &format!("/v1/operator/entities/{d}/erase"),
        Some(OPERATOR_KEY),
        Some(serde_json::json!({"operator":"alice","confirm_token":token})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(body["erased"], true);
    assert_eq!(body["kek_destroyed"], false);
    assert!(body["kek_skip_reason"]
        .as_str()
        .unwrap()
        .contains("no identified contribution"));
    // Gone.
    let (s, _) = req(
        &app,
        "GET",
        &format!("/v1/operator/entities/{d}"),
        Some(OPERATOR_KEY),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
