//! Stories 14.2 / 14.3 / 14.4 — happy-path coverage against live Postgres.
//!
//! Seeds a project + prompt + a handful of prompt runs + citations + a
//! mention, then GETs each of the three analytics endpoints under the
//! authenticated project's API key. Verifies:
//!
//! - citation-graph returns nodes/edges matching the seeded citations.
//! - heatmap returns at least one cell with a present brand.
//! - volatility returns a real Volatility payload (CV math doesn't
//!   crash on the seeded shape).
//!
//! Gated `#[ignore]` so default cargo runs stay offline. Run via:
//!
//! ```text
//! DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo \
//!   cargo test -p opengeo-api --test analytics_live_db -- --ignored
//! ```

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use opengeo_api::{router, AppState};
use opengeo_core::api_key::{generate as gen_key, API_KEY_HEADER};
use opengeo_core::{MentionId, ProjectId, PromptRunId};
use opengeo_storage::models::{MentionRow, ProjectRow, PromptRow, PromptRunRow};
use opengeo_storage::repositories::{
    api_keys::ApiKeyRepo, mentions::MentionRepo, projects::ProjectRepo, prompt_runs::PromptRunRepo,
    prompts::PromptRepo,
};
use sqlx::PgPool;
use tower::ServiceExt;

const BRAND: &str = "acme";

async fn seed() -> (axum::Router, String, PgPool) {
    let url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be exported for analytics_live_db");
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = Arc::new(opengeo_storage::Storage::from_pool(pool.clone()));
    let project_id = ProjectId::new();
    let now = Utc::now();

    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("test-{}", project_id),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");

    let prompt_id = opengeo_core::PromptId::new();
    PromptRepo::new(&pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: "vector-db".to_string(),
            text: "Best vector database?".to_string(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed prompt");

    // Two prompt runs (one per provider) so the analytics rows have
    // something to aggregate.
    let run_a = PromptRunId::new();
    let run_b = PromptRunId::new();
    let runs = [(run_a, "openai"), (run_b, "anthropic")];
    for (id, provider) in runs.iter() {
        PromptRunRepo::new(&pool)
            .insert(&PromptRunRow {
                id: *id,
                prompt_id,
                provider: provider.to_string(),
                provider_model_version: "test-1".to_string(),
                provider_region: None,
                started_at: now,
                finished_at: Some(now),
                raw_response: serde_json::json!({"x": 1}),
                request_parameters: serde_json::json!({}),
                status: "ok".to_string(),
                error_kind: None,
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await
            .expect("seed run");
    }

    // The openai run includes a brand mention at rank 2 so the heatmap
    // + volatility queries have something to read; the anthropic run
    // has none, exercising the None branch.
    MentionRepo::new(&pool)
        .insert(&MentionRow {
            id: MentionId::new(),
            prompt_run_id: run_a,
            entity: BRAND.to_string(),
            char_offset: 0,
            rank: 2,
            matched_text: BRAND.to_string(),
            sentiment_label: Some("neutral".to_string()),
            sentiment_score: Some(50),
            sentiment_lane: Some("deterministic_lexicon".to_string()),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed mention");

    // Citations: one openai → docs.acme.com, two openai → arxiv.org,
    // one anthropic → docs.acme.com. The graph should land with three
    // edges + one shared domain node.
    let cites = [
        (run_a, "docs.acme.com"),
        (run_a, "arxiv.org"),
        (run_a, "arxiv.org"),
        (run_b, "docs.acme.com"),
    ];
    for (run_id, domain) in cites.iter() {
        sqlx::query(
            r#"
            INSERT INTO citations (id, prompt_run_id, domain, frequency)
            VALUES ($1, $2, $3, 1)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(*run_id)
        .bind(*domain)
        .execute(&pool)
        .await
        .expect("seed citation");
    }

    let key = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(
            project_id,
            "fixture-key",
            &key.sha256_hash,
            &key.display_prefix,
        )
        .await
        .expect("seed api key");

    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id,
        events,
        config: None,
        provider_registry: None,
        configured_project: std::sync::Arc::new("default".to_string()),
        setup_install_state: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
                serve_info: None,
    };
    (router(state), key.plaintext, pool)
}

async fn get_json(app: &axum::Router, uri: &str, api_key: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(API_KEY_HEADER, api_key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    };
    (status, json)
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn citation_graph_returns_seeded_edges() {
    let (app, key, _pool) = seed().await;
    let (status, body) = get_json(&app, "/v1/analytics/citation-graph?days=7", &key).await;
    assert_eq!(status, StatusCode::OK);
    let nodes = body["nodes"].as_array().expect("nodes is array");
    let edges = body["edges"].as_array().expect("edges is array");
    // Expect at least 2 providers + 2 domains = 4 nodes; 3 edges
    // (openai→docs, openai→arxiv (weight 2), anthropic→docs).
    assert!(nodes.len() >= 4, "expected ≥4 nodes, got {}", nodes.len());
    assert_eq!(edges.len(), 3, "expected 3 unique provider×domain edges");
    let arxiv_edge = edges
        .iter()
        .find(|e| e["source"] == "openai" && e["target"] == "arxiv.org")
        .expect("openai → arxiv.org edge present");
    assert_eq!(arxiv_edge["weight"], 2);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn heatmap_returns_cells_with_brand_presence() {
    let (app, key, _pool) = seed().await;
    let (status, body) = get_json(
        &app,
        &format!("/v1/analytics/heatmap?brand={BRAND}&days=7"),
        &key,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cells = body["cells"].as_array().expect("cells is array");
    assert!(!cells.is_empty(), "expected at least one heatmap cell");
    // Find the openai cell — brand was present at rank 2 (presence_rate 1.0).
    let openai_cell = cells
        .iter()
        .find(|c| c["provider"] == "openai")
        .expect("openai cell present");
    assert_eq!(openai_cell["presence_rate"], serde_json::json!(1.0));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn volatility_returns_payload_shape() {
    let (app, key, _pool) = seed().await;
    let (status, body) = get_json(
        &app,
        &format!(
            "/v1/analytics/volatility?prompt=vector-db&provider=openai&brand={BRAND}&window=7"
        ),
        &key,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // Single observation → CV is 0.0 by definition.
    assert_eq!(body["samples"], serde_json::json!(7));
    assert_eq!(body["value"], serde_json::json!(0.0));
    let presence = body["presence_ratio"]
        .as_f64()
        .expect("presence_ratio is f64");
    assert!((0.0..=1.0).contains(&presence), "presence_ratio in [0,1]");
}

/// Per-provider analytics surfaces must split OpenRouter runs by their
/// upstream model. Two `openrouter` runs whose `provider_model_version`
/// differ are two distinct upstreams, so the aggregate/display queries
/// (which now group/select `pr.provider_identity`) must return them as
/// two separate rows keyed `openrouter:<model>` — not a single collapsed
/// `openrouter` row. Plain providers keep their bare name.
#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn openrouter_runs_split_by_upstream_model() {
    use opengeo_analytics::{citation_graph_rows, visibility_trend};

    let url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be exported for analytics_live_db");
    let pool = PgPool::connect(&url).await.expect("connect");
    let storage = opengeo_storage::Storage::from_pool(pool.clone());
    let project_id = ProjectId::new();
    let now = Utc::now();

    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("test-{}", project_id),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");

    let prompt_id = opengeo_core::PromptId::new();
    PromptRepo::new(&pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: "vector-db".to_string(),
            text: "Best vector database?".to_string(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed prompt");

    // Two OpenRouter runs against the same prompt but DIFFERENT upstream
    // models. `provider` is identical (`openrouter`); only the
    // `provider_model_version` distinguishes them. The generated
    // `provider_identity` column collapses to `openrouter:<model>`.
    let run_gpt = PromptRunId::new();
    let run_claude = PromptRunId::new();
    let runs = [
        (run_gpt, "openrouter", "openai/gpt-4o"),
        (run_claude, "openrouter", "anthropic/claude-3.5-sonnet"),
    ];
    for (id, provider, model) in runs.iter() {
        PromptRunRepo::new(&pool)
            .insert(&PromptRunRow {
                id: *id,
                prompt_id,
                provider: provider.to_string(),
                provider_model_version: model.to_string(),
                provider_region: None,
                started_at: now,
                finished_at: Some(now),
                raw_response: serde_json::json!({"x": 1}),
                request_parameters: serde_json::json!({}),
                status: "ok".to_string(),
                error_kind: None,
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await
            .expect("seed run");
    }

    // Two brand mentions (acme + beta) + a citation on each run so the
    // visibility, citation-graph, AND comparison aggregates each have a
    // row to emit per upstream. `beta` gives the comparison endpoint its
    // required second brand.
    for run_id in [run_gpt, run_claude] {
        for entity in [BRAND, "beta"] {
            MentionRepo::new(&pool)
                .insert(&MentionRow {
                    id: MentionId::new(),
                    prompt_run_id: run_id,
                    entity: entity.to_string(),
                    char_offset: 0,
                    rank: 2,
                    matched_text: entity.to_string(),
                    sentiment_label: Some("neutral".to_string()),
                    sentiment_score: Some(50),
                    sentiment_lane: Some("deterministic_lexicon".to_string()),
                    organization_id: None,
                    tenant_id: None,
                    created_at: now,
                })
                .await
                .expect("seed mention");
        }
        sqlx::query(
            r#"
            INSERT INTO citations (id, prompt_run_id, domain, frequency)
            VALUES ($1, $2, 'docs.acme.com', 1)
            "#,
        )
        .bind(uuid::Uuid::new_v4())
        .bind(run_id)
        .execute(&pool)
        .await
        .expect("seed citation");
    }

    let gpt_id = "openrouter:openai/gpt-4o";
    let claude_id = "openrouter:anthropic/claude-3.5-sonnet";

    // visibility_trend (GROUP BY provider_identity): two rows, one per
    // upstream identity — NOT one collapsed `openrouter` row.
    let trend = visibility_trend(&storage, project_id, "vector-db", 7)
        .await
        .expect("visibility_trend");
    let mut trend_providers: Vec<&str> = trend.iter().map(|p| p.provider.as_str()).collect();
    trend_providers.sort_unstable();
    trend_providers.dedup();
    assert_eq!(
        trend_providers,
        vec![claude_id, gpt_id],
        "visibility_trend must split the two OpenRouter upstreams"
    );
    assert!(
        !trend.iter().any(|p| p.provider == "openrouter"),
        "no bare `openrouter` row may survive the split"
    );

    // citation_graph_rows (GROUP BY provider_identity, domain): each
    // upstream contributes its own provider×domain edge.
    let cite_rows = citation_graph_rows(&storage, project_id, 7)
        .await
        .expect("citation_graph_rows");
    let mut cite_providers: Vec<&str> = cite_rows.iter().map(|r| r.provider.as_str()).collect();
    cite_providers.sort_unstable();
    cite_providers.dedup();
    assert_eq!(
        cite_providers,
        vec![claude_id, gpt_id],
        "citation_graph_rows must split the two OpenRouter upstreams"
    );
    assert!(
        cite_rows
            .iter()
            .any(|r| r.provider == gpt_id && r.domain == "docs.acme.com"),
        "gpt-4o upstream edge present"
    );
    assert!(
        cite_rows
            .iter()
            .any(|r| r.provider == claude_id && r.domain == "docs.acme.com"),
        "claude upstream edge present"
    );

    // Comparison matrix (GROUP BY ..., provider_identity): route-level
    // proof. Build the authenticated router and GET /v1/comparisons —
    // the two upstreams must surface as two rows keyed by identity.
    let key = gen_key();
    ApiKeyRepo::new(&pool)
        .insert(
            project_id,
            "fixture-key",
            &key.sha256_hash,
            &key.display_prefix,
        )
        .await
        .expect("seed api key");
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    let state = AppState {
        storage: Arc::new(storage),
        project_id,
        events,
        config: None,
        provider_registry: None,
        configured_project: std::sync::Arc::new("default".to_string()),
        setup_install_state: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
                serve_info: None,
    };
    let app = router(state);
    let (status, body) = get_json(
        &app,
        "/v1/comparisons?brands=acme,beta&window=7d",
        &key.plaintext,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "comparisons body: {body}");
    let rows = body["rows"].as_array().expect("rows is array");
    let mut row_providers: Vec<&str> = rows
        .iter()
        .map(|r| r["provider"].as_str().expect("provider str"))
        .collect();
    row_providers.sort_unstable();
    row_providers.dedup();
    assert_eq!(
        row_providers,
        vec![claude_id, gpt_id],
        "comparison matrix must split the two OpenRouter upstreams into two rows"
    );
    assert!(
        !rows.iter().any(|r| r["provider"] == "openrouter"),
        "no bare `openrouter` comparison row may survive the split"
    );
}
