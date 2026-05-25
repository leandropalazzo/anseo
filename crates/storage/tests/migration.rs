//! Story 1.3 migration smoke test (AC-7).
//!
//! Runs against an ephemeral schema created by `#[sqlx::test]` per the
//! architecture L604 convention. The body covers the full AC-7 checklist:
//!
//! 1. Column manifests for all five tables (AC-3) via `information_schema`.
//! 2. Tenant columns (`organization_id`, `tenant_id`) nullable on every table (AC-2).
//! 3. `created_at` columns default to `now()` (AC-2).
//! 4. `prompt_runs.status` CHECK rejects `'broken'`.
//! 5. `prompt_runs.error_kind` CHECK rejects `'not_a_real_kind'`.
//! 6. `status='ok'` `prompt_runs` row round-trips through `insert` + `get`.
//! 7. `status='failed'` row with `error_kind='provider_rate_limited'` round-trips.
//! 8. Deleting a `prompt_runs` row CASCADEs to `mentions` and `citations`.
//! 9. Deleting a `projects` row with surviving `prompts` is rejected (RESTRICT).

use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use opengeo_core::ids::{CitationId, MentionId, ProjectId, PromptId, PromptRunId};
use opengeo_storage::models::{CitationRow, MentionRow, ProjectRow, PromptRow, PromptRunRow};
use opengeo_storage::Storage;
use serde_json::json;
use sqlx::{PgPool, Row};

const PHASE1_TABLES: &[&str] = &[
    "projects",
    "prompts",
    "prompt_runs",
    "mentions",
    "citations",
];

/// (column_name, data_type, is_nullable, column_default-or-empty)
type ColumnExpectation = (&'static str, &'static str, &'static str, &'static str);

fn project_columns() -> Vec<ColumnExpectation> {
    vec![
        ("id", "uuid", "NO", ""),
        ("name", "text", "NO", ""),
        ("organization_id", "uuid", "YES", ""),
        ("tenant_id", "uuid", "YES", ""),
        ("created_at", "timestamp with time zone", "NO", "now()"),
    ]
}

fn prompt_columns() -> Vec<ColumnExpectation> {
    vec![
        ("id", "uuid", "NO", ""),
        ("project_id", "uuid", "NO", ""),
        ("name", "text", "NO", ""),
        ("text", "text", "NO", ""),
        ("organization_id", "uuid", "YES", ""),
        ("tenant_id", "uuid", "YES", ""),
        ("created_at", "timestamp with time zone", "NO", "now()"),
    ]
}

fn prompt_run_columns() -> Vec<ColumnExpectation> {
    vec![
        ("id", "uuid", "NO", ""),
        ("prompt_id", "uuid", "NO", ""),
        ("provider", "text", "NO", ""),
        ("provider_model_version", "text", "NO", ""),
        ("provider_region", "text", "YES", ""),
        ("started_at", "timestamp with time zone", "NO", ""),
        ("finished_at", "timestamp with time zone", "YES", ""),
        ("raw_response", "jsonb", "NO", "'{}'::jsonb"),
        ("request_parameters", "jsonb", "NO", "'{}'::jsonb"),
        ("status", "text", "NO", ""),
        ("error_kind", "text", "YES", ""),
        ("organization_id", "uuid", "YES", ""),
        ("tenant_id", "uuid", "YES", ""),
        ("created_at", "timestamp with time zone", "NO", "now()"),
    ]
}

fn mention_columns() -> Vec<ColumnExpectation> {
    vec![
        ("id", "uuid", "NO", ""),
        ("prompt_run_id", "uuid", "NO", ""),
        ("entity", "text", "NO", ""),
        ("char_offset", "integer", "NO", ""),
        ("rank", "integer", "NO", ""),
        ("matched_text", "text", "NO", ""),
        ("organization_id", "uuid", "YES", ""),
        ("tenant_id", "uuid", "YES", ""),
        ("created_at", "timestamp with time zone", "NO", "now()"),
    ]
}

fn citation_columns() -> Vec<ColumnExpectation> {
    vec![
        ("id", "uuid", "NO", ""),
        ("prompt_run_id", "uuid", "NO", ""),
        ("url", "text", "YES", ""),
        ("domain", "text", "NO", ""),
        ("frequency", "integer", "NO", "1"),
        ("source_type", "text", "YES", ""),
        ("organization_id", "uuid", "YES", ""),
        ("tenant_id", "uuid", "YES", ""),
        ("created_at", "timestamp with time zone", "NO", "now()"),
    ]
}

fn expectations_for(table: &str) -> Vec<ColumnExpectation> {
    match table {
        "projects" => project_columns(),
        "prompts" => prompt_columns(),
        "prompt_runs" => prompt_run_columns(),
        "mentions" => mention_columns(),
        "citations" => citation_columns(),
        other => panic!("unexpected table {other}"),
    }
}

async fn assert_column_manifest(pool: &PgPool, table: &str) {
    let rows = sqlx::query(
        r#"
        SELECT column_name, data_type, is_nullable, COALESCE(column_default, '') AS column_default
        FROM information_schema.columns
        WHERE table_schema = current_schema() AND table_name = $1
        ORDER BY ordinal_position
        "#,
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| panic!("column lookup for {table} failed: {e}"));

    let actual: HashMap<String, (String, String, String)> = rows
        .into_iter()
        .map(|r| {
            let name: String = r.get("column_name");
            let dtype: String = r.get("data_type");
            let nullable: String = r.get("is_nullable");
            let default: String = r.get("column_default");
            (name, (dtype, nullable, default))
        })
        .collect();

    let expected = expectations_for(table);
    assert_eq!(
        actual.len(),
        expected.len(),
        "table {table} column count mismatch: actual {actual:?}, expected {expected:?}"
    );

    for (col, dtype, nullable, default) in expected {
        let got = actual
            .get(col)
            .unwrap_or_else(|| panic!("table {table} missing column {col}"));
        assert_eq!(got.0, dtype, "table {table} column {col} data_type");
        assert_eq!(got.1, nullable, "table {table} column {col} nullable");
        if !default.is_empty() {
            assert!(
                got.2.contains(default),
                "table {table} column {col} default: expected to contain {default:?}, got {:?}",
                got.2,
            );
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn migration_creates_phase1_tables_and_round_trips_prompt_runs(pool: PgPool) {
    // AC-7.1: every Phase 1 table exists with the AC-3 column manifest.
    for table in PHASE1_TABLES {
        assert_column_manifest(&pool, table).await;
    }

    let storage = Storage::from_pool(pool.clone());

    // Seed a project + prompt for FK satisfaction.
    let project = ProjectRow {
        id: ProjectId::new(),
        name: "acme".into(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap(),
    };
    let project_id = storage.projects().insert(&project).await.unwrap();

    let prompt = PromptRow {
        id: PromptId::new(),
        project_id,
        name: "headline".into(),
        text: "Who makes the best widget?".into(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 1).unwrap(),
    };
    let prompt_id = storage.prompts().insert(&prompt).await.unwrap();

    // AC-7.6: status='ok' prompt_runs round-trip.
    let ok_run = PromptRunRow {
        id: PromptRunId::new(),
        prompt_id,
        provider: "openai".into(),
        provider_model_version: "gpt-4o-2026-05-01".into(),
        provider_region: Some("us-east-1".into()),
        started_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 2).unwrap(),
        finished_at: Some(Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 3).unwrap()),
        raw_response: json!({"text": "Acme Widgets, naturally."}),
        request_parameters: json!({"model": "gpt-4o-2026-05-01", "temperature": 0.2}),
        status: "ok".into(),
        error_kind: None,
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 2).unwrap(),
    };
    let ok_run_id = storage.prompt_runs().insert(&ok_run).await.unwrap();
    let fetched_ok = storage
        .prompt_runs()
        .get(ok_run_id)
        .await
        .unwrap()
        .expect("ok run must round-trip");
    assert_eq!(fetched_ok.id, ok_run.id);
    assert_eq!(fetched_ok.prompt_id, ok_run.prompt_id);
    assert_eq!(fetched_ok.provider, ok_run.provider);
    assert_eq!(
        fetched_ok.provider_model_version,
        ok_run.provider_model_version
    );
    assert_eq!(fetched_ok.provider_region, ok_run.provider_region);
    assert_eq!(fetched_ok.started_at, ok_run.started_at);
    assert_eq!(fetched_ok.finished_at, ok_run.finished_at);
    assert_eq!(fetched_ok.raw_response, ok_run.raw_response);
    assert_eq!(fetched_ok.request_parameters, ok_run.request_parameters);
    assert_eq!(fetched_ok.status, ok_run.status);
    assert_eq!(fetched_ok.error_kind, ok_run.error_kind);
    assert_eq!(fetched_ok.organization_id, ok_run.organization_id);
    assert_eq!(fetched_ok.tenant_id, ok_run.tenant_id);
    assert_eq!(fetched_ok.created_at, ok_run.created_at);

    // AC-7.7: status='failed' + error_kind='provider_rate_limited' round-trips.
    let failed_run = PromptRunRow {
        id: PromptRunId::new(),
        prompt_id,
        provider: "anthropic".into(),
        provider_model_version: "claude-opus-4-7-2026-05-01".into(),
        provider_region: None,
        started_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 1, 0).unwrap(),
        finished_at: None,
        raw_response: json!({}),
        request_parameters: json!({"model": "claude-opus-4-7-2026-05-01"}),
        status: "failed".into(),
        error_kind: Some("provider_rate_limited".into()),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 1, 0).unwrap(),
    };
    let failed_run_id = storage.prompt_runs().insert(&failed_run).await.unwrap();
    let fetched_failed = storage
        .prompt_runs()
        .get(failed_run_id)
        .await
        .unwrap()
        .expect("failed run must round-trip");
    assert_eq!(fetched_failed.status, "failed");
    assert_eq!(
        fetched_failed.error_kind.as_deref(),
        Some("provider_rate_limited")
    );

    // AC-7.4: status='broken' is rejected by the CHECK constraint.
    let bad_status = sqlx::query!(
        r#"
        INSERT INTO prompt_runs (
            id, prompt_id, provider, provider_model_version, started_at,
            raw_response, request_parameters, status, created_at
        )
        VALUES ($1, $2, 'x', 'y', now(), '{}'::jsonb, '{}'::jsonb, 'broken', now())
        "#,
        uuid::Uuid::new_v4(),
        prompt_id as PromptId,
    )
    .execute(&pool)
    .await;
    assert!(
        bad_status.is_err(),
        "INSERT with status='broken' must be rejected, got {:?}",
        bad_status,
    );

    // AC-7.5: error_kind='not_a_real_kind' is rejected by the CHECK constraint.
    let bad_kind = sqlx::query!(
        r#"
        INSERT INTO prompt_runs (
            id, prompt_id, provider, provider_model_version, started_at,
            raw_response, request_parameters, status, error_kind, created_at
        )
        VALUES (
            $1, $2, 'x', 'y', now(), '{}'::jsonb, '{}'::jsonb,
            'failed', 'not_a_real_kind', now()
        )
        "#,
        uuid::Uuid::new_v4(),
        prompt_id as PromptId,
    )
    .execute(&pool)
    .await;
    assert!(
        bad_kind.is_err(),
        "INSERT with error_kind='not_a_real_kind' must be rejected, got {:?}",
        bad_kind,
    );

    // AC-7.8: ON DELETE CASCADE on mentions + citations.
    let cascade_run = PromptRunRow {
        id: PromptRunId::new(),
        prompt_id,
        provider: "openai".into(),
        provider_model_version: "gpt-4o-2026-05-01".into(),
        provider_region: None,
        started_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 2, 0).unwrap(),
        finished_at: Some(Utc.with_ymd_and_hms(2026, 5, 25, 12, 2, 1).unwrap()),
        raw_response: json!({}),
        request_parameters: json!({}),
        status: "ok".into(),
        error_kind: None,
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 2, 0).unwrap(),
    };
    let cascade_run_id = storage.prompt_runs().insert(&cascade_run).await.unwrap();

    let mention = MentionRow {
        id: MentionId::new(),
        prompt_run_id: cascade_run_id,
        entity: "Acme".into(),
        char_offset: 0,
        rank: 1,
        matched_text: "Acme".into(),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 2, 1).unwrap(),
    };
    let mention_id = storage.mentions().insert(&mention).await.unwrap();

    let citation = CitationRow {
        id: CitationId::new(),
        prompt_run_id: cascade_run_id,
        url: Some("https://acme.example/docs".into()),
        domain: "acme.example".into(),
        frequency: 2,
        source_type: Some("docs".into()),
        organization_id: None,
        tenant_id: None,
        created_at: Utc.with_ymd_and_hms(2026, 5, 25, 12, 2, 1).unwrap(),
    };
    let citation_id = storage.citations().insert(&citation).await.unwrap();

    sqlx::query!(
        "DELETE FROM prompt_runs WHERE id = $1",
        cascade_run_id as PromptRunId,
    )
    .execute(&pool)
    .await
    .unwrap();

    assert!(
        storage.mentions().get(mention_id).await.unwrap().is_none(),
        "mention should have cascaded with its prompt_run",
    );
    assert!(
        storage
            .citations()
            .get(citation_id)
            .await
            .unwrap()
            .is_none(),
        "citation should have cascaded with its prompt_run",
    );

    // AC-7.9: ON DELETE RESTRICT on projects when prompts still reference them.
    let restrict_result = sqlx::query!(
        "DELETE FROM projects WHERE id = $1",
        project_id as ProjectId,
    )
    .execute(&pool)
    .await;
    assert!(
        restrict_result.is_err(),
        "deleting a project with surviving prompts must be rejected (RESTRICT), got {:?}",
        restrict_result,
    );
}
