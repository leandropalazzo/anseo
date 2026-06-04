//! Story 0.10 — `GET /v1/prompts/similarity-index` integration tests.
//!
//! These tests exercise the handler against a router built directly from
//! `routes::prompts_similarity::v1_router()` (i.e. WITHOUT the `/v1`
//! auth-key gate). The auth contract is covered globally in
//! `apps/api/tests/auth.rs` and `apps/api/tests/analytics.rs`; here we
//! focus on the handler-shape and the MinHash plumbing.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use opengeo_api::{routes::prompts_similarity, AppState};
use opengeo_core::config::{BrandConfig, Config, PromptConfig};
use opengeo_core::{AnomalySensitivity, ProjectId, SCHEMA_VERSION_V0_2};
use tower::ServiceExt;

fn lazy_storage() -> Arc<opengeo_storage::Storage> {
    let pool = sqlx::PgPool::connect_lazy(
        "postgres://opengeo:opengeo@127.0.0.1:1/__prompts_similarity_test__",
    )
    .expect("connect_lazy never IOs synchronously");
    Arc::new(opengeo_storage::Storage::from_pool(pool))
}

fn make_config(prompts: &[(&str, &str)]) -> Config {
    Config {
        schema_version: SCHEMA_VERSION_V0_2.to_string(),
        brand: BrandConfig {
            name: "Acme".to_string(),
            variants: Vec::new(),
            site_url: None,
        },
        competitors: Vec::new(),
        prompts: prompts
            .iter()
            .map(|(n, t)| PromptConfig {
                name: (*n).to_string(),
                text: (*t).to_string(),
                description: None,
            })
            .collect(),
        providers: Vec::new(),
        schedules: Vec::new(),
        concurrency: 4,
        anomaly_sensitivity: AnomalySensitivity::default(),
        analytics: None,
    }
}

fn build_app(config: Option<Config>) -> axum::Router {
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    let state = AppState {
        storage: lazy_storage(),
        project_id: ProjectId::new(),
        events,
        config: config.map(Arc::new),
        provider_registry: None,
        configured_project: Arc::new("default".to_string()),
        setup_install_state: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
    };
    prompts_similarity::v1_router().with_state(state)
}

async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let json =
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn exact_match_returns_one_with_full_jaccard() {
    let cfg = make_config(&[
        ("crm", "best crm for small business"),
        ("baking", "how to bake sourdough bread at home"),
    ]);
    let app = build_app(Some(cfg));
    let (status, body) = get_json(
        app,
        "/prompts/similarity-index?text=best%20crm%20for%20small%20business&threshold=0.5",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let matches = body["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["name"], "crm");
    let j = matches[0]["estimated_jaccard"].as_f64().unwrap();
    assert!((j - 1.0).abs() < 1e-5, "expected 1.0, got {j}");
    assert_eq!(matches[0]["rank_data_available"], true);
    assert_eq!(body["method"], "minhash");
    assert_eq!(body["num_hash_functions"], 128);
    assert!(body["input_hash"].as_str().unwrap().starts_with("sha256:"));
}

#[tokio::test]
async fn near_match_above_threshold_is_returned() {
    let cfg = make_config(&[("crm", "best crm for small business owners today")]);
    let app = build_app(Some(cfg));
    let (status, body) = get_json(
        app,
        "/prompts/similarity-index?text=best%20crm%20for%20small%20business%20owners%20now&threshold=0.5",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let matches = body["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    let j = matches[0]["estimated_jaccard"].as_f64().unwrap();
    assert!(
        (0.5..1.0).contains(&j),
        "expected near-match in [0.5,1.0), got {j}"
    );
}

#[tokio::test]
async fn far_match_returns_empty() {
    let cfg = make_config(&[("crm", "best crm for small business")]);
    let app = build_app(Some(cfg));
    let (status, body) = get_json(
        app,
        "/prompts/similarity-index?text=how%20to%20bake%20sourdough%20bread%20in%20winter&threshold=0.6",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let matches = body["matches"].as_array().unwrap();
    assert!(matches.is_empty(), "expected no matches, got {matches:?}");
}

#[tokio::test]
async fn empty_text_returns_400() {
    let app = build_app(Some(make_config(&[("a", "x")])));
    let (status, _) = get_json(app, "/prompts/similarity-index?text=").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oversized_text_returns_400() {
    let app = build_app(Some(make_config(&[("a", "x")])));
    let big = "a".repeat(4097);
    let uri = format!("/prompts/similarity-index?text={big}");
    let (status, _) = get_json(app, &uri).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn out_of_range_threshold_returns_400() {
    let app = build_app(Some(make_config(&[("a", "x")])));
    let (status, _) = get_json(app, "/prompts/similarity-index?text=hi&threshold=1.5").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn no_config_returns_ok_with_empty_matches() {
    let app = build_app(None);
    let (status, body) = get_json(app, "/prompts/similarity-index?text=hello%20world").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["matches"].as_array().unwrap().is_empty());
    assert_eq!(body["method"], "minhash");
}
