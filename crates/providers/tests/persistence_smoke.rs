//! Integration test for Story 3.1 — orchestrator records persist into
//! Postgres and round-trip through the storage repositories.
//!
//! Uses `#[sqlx::test]` so sqlx hands us an ephemeral schema with the
//! migration already applied.

use std::collections::HashMap;
use std::sync::Arc;

use opengeo_core::{Config, ProviderErrorKind, ProviderName};
use opengeo_providers::{
    persistence::persist_records, MockProvider, Orchestrator, OrchestratorFilter, ProviderError,
    ProviderRegistry,
};
use opengeo_storage::Storage;
use sqlx::PgPool;

const YAML: &str = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: p1
    text: "first prompt"
  - name: p2
    text: "second prompt"
providers:
  - name: openai
    model: mock-model
  - name: anthropic
    model: mock-model
"#;

#[sqlx::test(migrations = "../storage/migrations")]
async fn full_matrix_persists_with_one_failure_and_three_successes(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let cfg = Config::from_yaml_str(YAML).unwrap();

    // Build registry: openai's first call fails; everything else succeeds.
    let openai = MockProvider::new(ProviderName::Openai)
        .accept_model("mock-model")
        .queue_failure(ProviderError::rate_limited("429"))
        .queue_response("openai-ok");
    let anthropic = MockProvider::new(ProviderName::Anthropic)
        .accept_model("mock-model")
        .queue_response("anthropic-ok-1")
        .queue_response("anthropic-ok-2");
    let mut registry: ProviderRegistry = HashMap::new();
    registry.insert(ProviderName::Openai, Arc::new(openai));
    registry.insert(ProviderName::Anthropic, Arc::new(anthropic));

    let orchestrator = Orchestrator::new(cfg.clone(), registry);
    let records = orchestrator.run_all(OrchestratorFilter::default()).await;
    assert_eq!(records.len(), 4);

    let persisted = persist_records(&storage, &cfg, &records).await.unwrap();
    assert_eq!(persisted.len(), 4);

    // Round-trip every record.
    for p in &persisted {
        let row = storage
            .prompt_runs()
            .get(p.run_id)
            .await
            .unwrap()
            .expect("inserted row should be readable");
        assert_eq!(row.id, p.run_id);
    }

    // Verify status counts via direct SQL.
    let ok_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM prompt_runs WHERE status='ok'")
        .fetch_one(storage.pool())
        .await
        .unwrap()
        .unwrap_or(0);
    let failed_count: i64 =
        sqlx::query_scalar!("SELECT COUNT(*) FROM prompt_runs WHERE status='failed'")
            .fetch_one(storage.pool())
            .await
            .unwrap()
            .unwrap_or(0);
    assert_eq!(ok_count, 3);
    assert_eq!(failed_count, 1);

    // The failed row must carry the closed-set error_kind.
    let kinds: Vec<Option<String>> =
        sqlx::query_scalar!("SELECT error_kind FROM prompt_runs WHERE status='failed'")
            .fetch_all(storage.pool())
            .await
            .unwrap();
    assert_eq!(
        kinds.first().and_then(|s| s.clone()).as_deref(),
        Some(ProviderErrorKind::ProviderRateLimited.as_wire_str())
    );

    // projects + prompts upserts: 1 project, 2 prompts.
    let project_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM projects")
        .fetch_one(storage.pool())
        .await
        .unwrap()
        .unwrap_or(0);
    assert_eq!(project_count, 1);
    let prompt_count: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM prompts")
        .fetch_one(storage.pool())
        .await
        .unwrap()
        .unwrap_or(0);
    assert_eq!(prompt_count, 2);
}

#[sqlx::test(migrations = "../storage/migrations")]
async fn rerun_does_not_lose_history(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let cfg = Config::from_yaml_str(YAML).unwrap();

    for round in 0..2 {
        let openai = MockProvider::new(ProviderName::Openai)
            .accept_model("mock-model")
            .queue_response(format!("openai-r{round}-p1"))
            .queue_response(format!("openai-r{round}-p2"));
        let anthropic = MockProvider::new(ProviderName::Anthropic)
            .accept_model("mock-model")
            .queue_response(format!("anthropic-r{round}-p1"))
            .queue_response(format!("anthropic-r{round}-p2"));
        let mut registry: ProviderRegistry = HashMap::new();
        registry.insert(ProviderName::Openai, Arc::new(openai));
        registry.insert(ProviderName::Anthropic, Arc::new(anthropic));
        let records = Orchestrator::new(cfg.clone(), registry)
            .run_all(OrchestratorFilter::default())
            .await;
        persist_records(&storage, &cfg, &records).await.unwrap();
    }

    let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM prompt_runs")
        .fetch_one(storage.pool())
        .await
        .unwrap()
        .unwrap_or(0);
    assert_eq!(total, 8, "every re-run appends new rows (FR-6)");
}

/// Story 31-3 — `persist_records` records a provenance trail per run: a
/// `provider_call` step (ok/error), `response_persisted`, and the three
/// skipped extraction/ranking stages. ≥3 rows per run; ordered by `at`.
#[sqlx::test(migrations = "../storage/migrations")]
async fn persist_records_writes_provenance_trail(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let cfg = Config::from_yaml_str(YAML).unwrap();

    // One failing provider call (openai p1) and the rest succeeding.
    let openai = MockProvider::new(ProviderName::Openai)
        .accept_model("mock-model")
        .queue_failure(ProviderError::rate_limited("429"))
        .queue_response("openai-ok");
    let anthropic = MockProvider::new(ProviderName::Anthropic)
        .accept_model("mock-model")
        .queue_response("anthropic-ok-1")
        .queue_response("anthropic-ok-2");
    let mut registry: ProviderRegistry = HashMap::new();
    registry.insert(ProviderName::Openai, Arc::new(openai));
    registry.insert(ProviderName::Anthropic, Arc::new(anthropic));

    let records = Orchestrator::new(cfg.clone(), registry)
        .run_all(OrchestratorFilter::default())
        .await;
    let persisted = persist_records(&storage, &cfg, &records).await.unwrap();
    assert_eq!(persisted.len(), 4);

    // Every run gets exactly 5 provenance rows (provider_call,
    // response_persisted, mention_extraction, citation_extraction, ranking).
    let total_prov: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM run_provenance")
        .fetch_one(storage.pool())
        .await
        .unwrap()
        .unwrap_or(0);
    assert_eq!(total_prov, 4 * 5);

    // The single failed run records provider_call=error; the rest are ok.
    let provider_call_errors: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM run_provenance WHERE step='provider_call' AND status='error'"
    )
    .fetch_one(storage.pool())
    .await
    .unwrap()
    .unwrap_or(0);
    assert_eq!(provider_call_errors, 1);

    // Spot-check one run's trail via the repo, asserting ordered steps.
    let any_run = persisted[0].run_id;
    let steps = storage.run_provenance().list_by_run(any_run).await.unwrap();
    let names: Vec<&str> = steps.iter().map(|s| s.step.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "provider_call",
            "response_persisted",
            "mention_extraction",
            "citation_extraction",
            "ranking",
        ]
    );
    assert_eq!(steps[1].status, "ok"); // response_persisted always ok
    assert_eq!(steps[4].status, "skipped"); // ranking skipped (3.2 pending)
}
