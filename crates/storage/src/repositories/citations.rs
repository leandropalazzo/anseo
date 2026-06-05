use anseo_core::ids::{CitationId, PromptRunId};
use sqlx::PgPool;

use crate::error::Error;
use crate::models::CitationRow;

pub struct CitationRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> CitationRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, row: &CitationRow) -> Result<CitationId, Error> {
        sqlx::query!(
            r#"
            INSERT INTO citations (
                id, prompt_run_id, url, domain, frequency, source_type,
                organization_id, tenant_id, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            row.id as CitationId,
            row.prompt_run_id as PromptRunId,
            row.url,
            row.domain,
            row.frequency,
            row.source_type,
            row.organization_id,
            row.tenant_id,
            row.created_at,
        )
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    /// Citations attached to a Prompt Run (Story 19.6 EngineInput assembly).
    /// Runtime `query_as` to keep the offline `.sqlx/` cache untouched.
    pub async fn list_by_run(&self, prompt_run_id: PromptRunId) -> Result<Vec<CitationRow>, Error> {
        let rid = uuid::Uuid::from_bytes(prompt_run_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, CitationRow>(
            r#"
            SELECT id, prompt_run_id, url, domain, frequency, source_type,
                   organization_id, tenant_id, created_at
            FROM citations
            WHERE prompt_run_id = $1
            ORDER BY id
            "#,
        )
        .bind(rid)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get(&self, id: CitationId) -> Result<Option<CitationRow>, Error> {
        let row = sqlx::query_as!(
            CitationRow,
            r#"
            SELECT
                id              AS "id: CitationId",
                prompt_run_id   AS "prompt_run_id: PromptRunId",
                url,
                domain,
                frequency,
                source_type,
                organization_id,
                tenant_id,
                created_at
            FROM citations
            WHERE id = $1
            "#,
            id as CitationId,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}
