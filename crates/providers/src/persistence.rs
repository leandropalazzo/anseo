//! Persistence glue from [`PromptRunRecord`] to the `crates/storage`
//! repositories (Story 3.1).
//!
//! The orchestrator emits in-memory records; this module writes them through
//! the typed repositories. It also takes responsibility for upserting the
//! parent rows (projects, prompts) so a fresh database can absorb a run
//! without an explicit setup step. Idempotency is via the deterministic
//! `project_id` / `prompt_id` derivation from Story 2.1.
//!
//! # Order of writes
//!
//! Each `(prompt, provider)` cell touches up to three tables:
//!
//! 1. `projects` — upsert by stable id (`Config::project_id`).
//! 2. `prompts`  — upsert by stable id (`Config::prompt_id`).
//! 3. `prompt_runs` — always inserted; status `ok` or `failed`.
//!
//! Mentions and Citations are persisted by Stories 3.2 / 3.3 once the
//! extractor crate populates them.

use chrono::Utc;
use opengeo_core::Config;
use opengeo_storage::models::{ProjectRow, PromptRow, PromptRunRow};
use opengeo_storage::Storage;

use crate::orchestrator::{PromptRunRecord, PromptRunStatus};

pub struct PersistedRun {
    pub run_id: opengeo_core::PromptRunId,
    pub status: PromptRunStatus,
}

/// Ensure the project and prompts referenced by `config` exist in the DB,
/// then persist every record. Returns the inserted run IDs in input order.
pub async fn persist_records(
    storage: &Storage,
    config: &Config,
    records: &[PromptRunRecord],
) -> Result<Vec<PersistedRun>, opengeo_storage::Error> {
    // 1. Project upsert.
    let project_id = config.project_id();
    upsert_project(
        storage,
        &ProjectRow {
            id: project_id,
            name: config.brand.name.clone(),
            organization_id: None,
            tenant_id: None,
            created_at: Utc::now(),
        },
    )
    .await?;

    // 2. Prompt upserts.
    for prompt in &config.prompts {
        let prompt_id = config
            .prompt_id(&prompt.name)
            .expect("declared prompts always resolve");
        upsert_prompt(
            storage,
            &PromptRow {
                id: prompt_id,
                project_id,
                name: prompt.name.clone(),
                text: prompt.text.clone(),
                organization_id: None,
                tenant_id: None,
                created_at: Utc::now(),
            },
        )
        .await?;
    }

    // 3. Prompt run inserts.
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let row = PromptRunRow {
            id: record.id,
            prompt_id: record.prompt_id,
            provider: record.provider.as_wire_str().to_string(),
            provider_model_version: record.provider_model_version.clone(),
            provider_region: record.provider_region.clone(),
            started_at: record.started_at,
            finished_at: record.finished_at,
            raw_response: record.raw_response.clone(),
            request_parameters: record.request_parameters.clone(),
            status: record.status.as_wire_str().to_string(),
            error_kind: record.error_kind.map(|k| k.as_wire_str().to_string()),
            organization_id: None,
            tenant_id: None,
            created_at: Utc::now(),
        };
        let id = storage.prompt_runs().insert(&row).await?;
        out.push(PersistedRun {
            run_id: id,
            status: record.status,
        });
    }
    Ok(out)
}

async fn upsert_project(storage: &Storage, row: &ProjectRow) -> Result<(), opengeo_storage::Error> {
    use opengeo_core::ProjectId;
    sqlx::query!(
        r#"
        INSERT INTO projects (id, name, organization_id, tenant_id, created_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name
        "#,
        row.id as ProjectId,
        row.name,
        row.organization_id,
        row.tenant_id,
        row.created_at,
    )
    .execute(storage.pool())
    .await?;
    Ok(())
}

async fn upsert_prompt(storage: &Storage, row: &PromptRow) -> Result<(), opengeo_storage::Error> {
    use opengeo_core::{ProjectId, PromptId};
    sqlx::query!(
        r#"
        INSERT INTO prompts (id, project_id, name, text, organization_id, tenant_id, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (id) DO UPDATE SET text = EXCLUDED.text
        "#,
        row.id as PromptId,
        row.project_id as ProjectId,
        row.name,
        row.text,
        row.organization_id,
        row.tenant_id,
        row.created_at,
    )
    .execute(storage.pool())
    .await?;
    Ok(())
}
