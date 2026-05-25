use opengeo_core::ids::{MentionId, PromptRunId};
use sqlx::PgPool;

use crate::error::Error;
use crate::models::MentionRow;

pub struct MentionRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> MentionRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, row: &MentionRow) -> Result<MentionId, Error> {
        sqlx::query!(
            r#"
            INSERT INTO mentions (
                id, prompt_run_id, entity, char_offset, rank, matched_text,
                organization_id, tenant_id, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            row.id as MentionId,
            row.prompt_run_id as PromptRunId,
            row.entity,
            row.char_offset,
            row.rank,
            row.matched_text,
            row.organization_id,
            row.tenant_id,
            row.created_at,
        )
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn get(&self, id: MentionId) -> Result<Option<MentionRow>, Error> {
        let row = sqlx::query_as!(
            MentionRow,
            r#"
            SELECT
                id              AS "id: MentionId",
                prompt_run_id   AS "prompt_run_id: PromptRunId",
                entity,
                char_offset,
                rank,
                matched_text,
                organization_id,
                tenant_id,
                created_at
            FROM mentions
            WHERE id = $1
            "#,
            id as MentionId,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}
