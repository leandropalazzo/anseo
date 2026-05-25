use opengeo_core::ids::{PromptId, PromptRunId};
use sqlx::PgPool;

use crate::error::Error;
use crate::models::PromptRunRow;

pub struct PromptRunRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> PromptRunRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, row: &PromptRunRow) -> Result<PromptRunId, Error> {
        sqlx::query!(
            r#"
            INSERT INTO prompt_runs (
                id,
                prompt_id,
                provider,
                provider_model_version,
                provider_region,
                started_at,
                finished_at,
                raw_response,
                request_parameters,
                status,
                error_kind,
                organization_id,
                tenant_id,
                created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
            "#,
            row.id as PromptRunId,
            row.prompt_id as PromptId,
            row.provider,
            row.provider_model_version,
            row.provider_region,
            row.started_at,
            row.finished_at,
            row.raw_response,
            row.request_parameters,
            row.status,
            row.error_kind,
            row.organization_id,
            row.tenant_id,
            row.created_at,
        )
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn get(&self, id: PromptRunId) -> Result<Option<PromptRunRow>, Error> {
        let row = sqlx::query_as!(
            PromptRunRow,
            r#"
            SELECT
                id                     AS "id: PromptRunId",
                prompt_id              AS "prompt_id: PromptId",
                provider,
                provider_model_version,
                provider_region,
                started_at,
                finished_at,
                raw_response,
                request_parameters,
                status,
                error_kind,
                organization_id,
                tenant_id,
                created_at
            FROM prompt_runs
            WHERE id = $1
            "#,
            id as PromptRunId,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}
