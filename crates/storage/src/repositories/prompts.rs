use opengeo_core::ids::{ProjectId, PromptId};
use sqlx::PgPool;

use crate::error::Error;
use crate::models::PromptRow;

pub struct PromptRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> PromptRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, row: &PromptRow) -> Result<PromptId, Error> {
        sqlx::query!(
            r#"
            INSERT INTO prompts (id, project_id, name, text, organization_id, tenant_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            row.id as PromptId,
            row.project_id as ProjectId,
            row.name,
            row.text,
            row.organization_id,
            row.tenant_id,
            row.created_at,
        )
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    /// Story 12.2 — look up a Prompt by its slug-safe `name` scoped to a
    /// project. Returns `None` if the project hasn't declared a prompt by
    /// that name (the API write handler uses this to 404 a request that
    /// references an undeclared prompt).
    pub async fn find_by_name(
        &self,
        project_id: ProjectId,
        name: &str,
    ) -> Result<Option<PromptRow>, Error> {
        let row = sqlx::query_as!(
            PromptRow,
            r#"
            SELECT
                id              AS "id: PromptId",
                project_id      AS "project_id: ProjectId",
                name,
                text,
                organization_id,
                tenant_id,
                created_at
            FROM prompts
            WHERE project_id = $1 AND name = $2
            "#,
            project_id as ProjectId,
            name,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get(&self, id: PromptId) -> Result<Option<PromptRow>, Error> {
        let row = sqlx::query_as!(
            PromptRow,
            r#"
            SELECT
                id              AS "id: PromptId",
                project_id      AS "project_id: ProjectId",
                name,
                text,
                organization_id,
                tenant_id,
                created_at
            FROM prompts
            WHERE id = $1
            "#,
            id as PromptId,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}
