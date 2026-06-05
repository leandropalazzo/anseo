use anseo_core::ids::{ProjectId, PromptId};
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

    // Runtime queries (not the compile-time macros) so the offline `.sqlx/`
    // cache stays untouched when the `tags` column is added, matching the
    // `list_by_project` pattern.
    pub async fn insert(&self, row: &PromptRow) -> Result<PromptId, Error> {
        let pid = uuid::Uuid::from_bytes(row.id.into_ulid().to_bytes());
        let proj = uuid::Uuid::from_bytes(row.project_id.into_ulid().to_bytes());
        sqlx::query(
            r#"
            INSERT INTO prompts (id, project_id, name, text, tags, organization_id, tenant_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(pid)
        .bind(proj)
        .bind(&row.name)
        .bind(&row.text)
        .bind(&row.tags)
        .bind(row.organization_id)
        .bind(row.tenant_id)
        .bind(row.created_at)
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
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let row = sqlx::query_as::<_, PromptRow>(
            r#"
            SELECT id, project_id, name, text, tags, organization_id, tenant_id, created_at
            FROM prompts
            WHERE project_id = $1 AND name = $2
            "#,
        )
        .bind(pid)
        .bind(name)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// All prompts declared for a project (Story 19.6 EngineInput assembly).
    /// Runtime `query_as` (not the compile-time macro) so the offline `.sqlx/`
    /// cache stays untouched, matching the `recommendations` repo pattern.
    pub async fn list_by_project(&self, project_id: ProjectId) -> Result<Vec<PromptRow>, Error> {
        let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        let rows = sqlx::query_as::<_, PromptRow>(
            r#"
            SELECT id, project_id, name, text, tags, organization_id, tenant_id, created_at
            FROM prompts
            WHERE project_id = $1
            ORDER BY created_at, id
            "#,
        )
        .bind(pid)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Count `prompt_runs` for a single prompt. Gates a rename: re-deriving a
    /// prompt id is only safe when nothing references the old id, mirroring the
    /// brand rename rule on the project.
    pub async fn prompt_run_count(&self, id: PromptId) -> Result<i64, Error> {
        let n: i64 = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!" FROM prompt_runs WHERE prompt_id = $1"#,
            id as PromptId,
        )
        .fetch_one(self.pool)
        .await?;
        Ok(n)
    }

    /// Update a prompt's text + tags in place WITHOUT changing identity. Used
    /// when the prompt name is unchanged (only the body/tags edited).
    pub async fn update_content(
        &self,
        id: PromptId,
        text: &str,
        tags: &[String],
    ) -> Result<(), Error> {
        let pid = uuid::Uuid::from_bytes(id.into_ulid().to_bytes());
        sqlx::query(r#"UPDATE prompts SET text = $2, tags = $3 WHERE id = $1"#)
            .bind(pid)
            .bind(text)
            .bind(tags)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    /// Re-key a prompt to a new id (rename) — only valid when the prompt has no
    /// `prompt_runs` (the caller must check [`prompt_run_count`]). Updates id +
    /// name + text in one statement; the new id is derived from the brand name
    /// folded with the new prompt name.
    pub async fn rename_on_empty(
        &self,
        old_id: PromptId,
        new_id: PromptId,
        new_name: &str,
        text: &str,
        tags: &[String],
    ) -> Result<(), Error> {
        let new_pid = uuid::Uuid::from_bytes(new_id.into_ulid().to_bytes());
        let old_pid = uuid::Uuid::from_bytes(old_id.into_ulid().to_bytes());
        sqlx::query(r#"UPDATE prompts SET id = $1, name = $2, text = $3, tags = $4 WHERE id = $5"#)
            .bind(new_pid)
            .bind(new_name)
            .bind(text)
            .bind(tags)
            .bind(old_pid)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete(&self, id: PromptId) -> Result<(), Error> {
        sqlx::query!(r#"DELETE FROM prompts WHERE id = $1"#, id as PromptId)
            .execute(self.pool)
            .await?;
        Ok(())
    }

    pub async fn get(&self, id: PromptId) -> Result<Option<PromptRow>, Error> {
        let pid = uuid::Uuid::from_bytes(id.into_ulid().to_bytes());
        let row = sqlx::query_as::<_, PromptRow>(
            r#"
            SELECT id, project_id, name, text, tags, organization_id, tenant_id, created_at
            FROM prompts
            WHERE id = $1
            "#,
        )
        .bind(pid)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}
