use anseo_core::ids::{ClaimId, GroundTruthFactId, ProjectId, PromptId, PromptRunId};
use anseo_storage::models::{
    ExtractedClaimRow, GroundTruthFactRow, ProjectRow, PromptRow, PromptRunRow,
};
use anseo_storage::Storage;
use chrono::{TimeZone, Utc};
use serde_json::json;
use sqlx::PgPool;

async fn seed_run(pool: PgPool) -> (Storage, ProjectId, PromptRunId) {
    let storage = Storage::from_pool(pool);
    let project = ProjectRow {
        id: ProjectId::new(),
        name: "Acme".into(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 6, 2, 11, 0, 0).unwrap(),
    };
    let project_id = storage.projects().insert(&project).await.unwrap();

    let prompt = PromptRow {
        id: PromptId::new(),
        project_id,
        name: "accuracy".into(),
        text: "What is Acme?".into(),
        tags: Vec::new(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 6, 2, 11, 0, 1).unwrap(),
    };
    let prompt_id = storage.prompts().insert(&prompt).await.unwrap();

    let run = PromptRunRow {
        id: PromptRunId::new(),
        prompt_id,
        provider: "openai".into(),
        provider_model_version: "gpt-4o-2026-05-01".into(),
        provider_region: None,
        started_at: Utc.with_ymd_and_hms(2026, 6, 2, 11, 0, 2).unwrap(),
        finished_at: Some(Utc.with_ymd_and_hms(2026, 6, 2, 11, 0, 3).unwrap()),
        raw_response: json!({"text": "Acme is headquartered in Amsterdam."}),
        request_parameters: json!({}),
        status: "ok".into(),
        error_kind: None,
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 6, 2, 11, 0, 2).unwrap(),
    };
    let run_id = storage.prompt_runs().insert(&run).await.unwrap();

    (storage, project_id, run_id)
}

#[sqlx::test(migrations = "./migrations")]
async fn claims_link_to_prompt_run_and_cascade(pool: PgPool) {
    let (storage, _project_id, run_id) = seed_run(pool.clone()).await;
    let claim = ExtractedClaimRow {
        id: ClaimId::new(),
        prompt_run_id: run_id,
        entity: "Acme".into(),
        claim_text: "Acme is headquartered in Amsterdam.".into(),
        claim_kind: "factual_statement".into(),
        char_offset: Some(0),
        confidence: 80,
        extractor_lane: "deterministic_sentence".into(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 6, 2, 11, 0, 4).unwrap(),
    };
    let claim_id = storage.brand_accuracy().insert_claim(&claim).await.unwrap();

    let claims = storage
        .brand_accuracy()
        .list_claims_by_run(run_id)
        .await
        .unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].id, claim_id);
    assert_eq!(claims[0].prompt_run_id, run_id);
    assert_eq!(claims[0].claim_text, claim.claim_text);

    sqlx::query("DELETE FROM prompt_runs WHERE id = $1")
        .bind(uuid::Uuid::from_bytes(run_id.into_ulid().to_bytes()))
        .execute(&pool)
        .await
        .unwrap();
    assert!(storage
        .brand_accuracy()
        .get_claim(claim_id)
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn ground_truth_fact_round_trips_and_upserts(pool: PgPool) {
    let (storage, project_id, _run_id) = seed_run(pool).await;
    let fact = GroundTruthFactRow {
        id: GroundTruthFactId::new(),
        project_id,
        entity: "Acme".into(),
        fact_key: "headquarters".into(),
        fact_value: "Amsterdam".into(),
        source_url: Some("https://acme.example/about".into()),
        source_label: Some("Acme about page".into()),
        source_type: Some("official_site".into()),
        valid_from: None,
        valid_to: None,
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 6, 2, 11, 1, 0).unwrap(),
    };

    storage
        .brand_accuracy()
        .upsert_ground_truth_fact(&fact)
        .await
        .unwrap();
    let rows = storage
        .brand_accuracy()
        .list_ground_truth_for_entity(project_id, "Acme")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fact_key, "headquarters");
    assert_eq!(rows[0].fact_value, "Amsterdam");
    assert_eq!(rows[0].source_type.as_deref(), Some("official_site"));

    let updated = GroundTruthFactRow {
        fact_value: "Amsterdam, Netherlands".into(),
        ..fact
    };
    storage
        .brand_accuracy()
        .upsert_ground_truth_fact(&updated)
        .await
        .unwrap();
    let rows = storage
        .brand_accuracy()
        .list_ground_truth_for_entity(project_id, "Acme")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].fact_value, "Amsterdam, Netherlands");
}
