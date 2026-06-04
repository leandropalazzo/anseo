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
        sqlx::query(
            r#"
            INSERT INTO mentions (
                id, prompt_run_id, entity, char_offset, rank, matched_text,
                sentiment_label, sentiment_score, sentiment_lane,
                organization_id, tenant_id, created_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(row.id)
        .bind(row.prompt_run_id)
        .bind(&row.entity)
        .bind(row.char_offset)
        .bind(row.rank)
        .bind(&row.matched_text)
        .bind(&row.sentiment_label)
        .bind(row.sentiment_score)
        .bind(&row.sentiment_lane)
        .bind(row.organization_id)
        .bind(row.tenant_id)
        .bind(row.created_at)
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    /// Mentions attached to a Prompt Run (Story 30-6 run-detail surface).
    /// Runtime `query_as` to keep the offline `.sqlx/` cache untouched.
    pub async fn list_by_run(&self, prompt_run_id: PromptRunId) -> Result<Vec<MentionRow>, Error> {
        let rid = uuid::Uuid::from_bytes(prompt_run_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, MentionRow>(
            r#"
            SELECT id, prompt_run_id, entity, char_offset, rank, matched_text,
                   sentiment_label, sentiment_score, sentiment_lane,
                   organization_id, tenant_id, created_at
            FROM mentions
            WHERE prompt_run_id = $1
            ORDER BY rank, id
            "#,
        )
        .bind(rid)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get(&self, id: MentionId) -> Result<Option<MentionRow>, Error> {
        let mid = uuid::Uuid::from_bytes(id.into_ulid().to_bytes());
        let row = sqlx::query_as::<_, MentionRow>(
            r#"
            SELECT
                id,
                prompt_run_id,
                entity,
                char_offset,
                rank,
                matched_text,
                sentiment_label,
                sentiment_score,
                sentiment_lane,
                organization_id,
                tenant_id,
                created_at
            FROM mentions
            WHERE id = $1
            "#,
        )
        .bind(mid)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}
