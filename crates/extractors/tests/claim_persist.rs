use chrono::{TimeZone, Utc};
use opengeo_core::ids::{ProjectId, PromptId, PromptRunId};
use opengeo_storage::models::{ProjectRow, PromptRow, PromptRunRow};
use opengeo_storage::Storage;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "../../crates/storage/migrations")]
async fn persisted_brand_response_extracts_and_stores_claims(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let config = opengeo_extractors::mentions::config_with("Acme", &["Beta Corp"]);
    let now = Utc.with_ymd_and_hms(2026, 6, 2, 11, 30, 0).unwrap();

    let project_id = storage
        .projects()
        .insert(&ProjectRow {
            id: ProjectId::new(),
            name: "Acme".into(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .unwrap();
    let prompt_id = storage
        .prompts()
        .insert(&PromptRow {
            id: PromptId::new(),
            project_id,
            name: "accuracy".into(),
            text: "What is Acme?".into(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .unwrap();
    let run_id = storage
        .prompt_runs()
        .insert(&PromptRunRow {
            id: PromptRunId::new(),
            prompt_id,
            provider: "openai".into(),
            provider_model_version: "gpt-4o-2026-05-01".into(),
            provider_region: None,
            started_at: now,
            finished_at: Some(now),
            raw_response: json!({"text": "Acme is headquartered in Amsterdam."}),
            request_parameters: json!({}),
            status: "ok".into(),
            error_kind: None,
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .unwrap();

    let (_mentions, _citations, claims) = opengeo_extractors::extract_and_persist(
        &storage,
        &config,
        run_id,
        "Acme is headquartered in Amsterdam.",
        &json!({}),
        now,
    )
    .await
    .unwrap();

    let stored = storage
        .brand_accuracy()
        .list_claims_by_run(run_id)
        .await
        .unwrap();
    assert_eq!(claims, 1);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].entity, "Acme");
    assert_eq!(stored[0].claim_text, "Acme is headquartered in Amsterdam.");
}
