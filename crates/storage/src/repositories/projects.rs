use opengeo_core::ids::ProjectId;
use sqlx::PgPool;

use crate::error::Error;
use crate::models::ProjectRow;

pub struct ProjectRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ProjectRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, row: &ProjectRow) -> Result<ProjectId, Error> {
        sqlx::query!(
            r#"
            INSERT INTO projects (id, name, organization_id, tenant_id, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            row.id as ProjectId,
            row.name,
            row.organization_id,
            row.tenant_id,
            row.created_at,
        )
        .execute(self.pool)
        .await?;
        Ok(row.id)
    }

    pub async fn get(&self, id: ProjectId) -> Result<Option<ProjectRow>, Error> {
        let row = sqlx::query_as!(
            ProjectRow,
            r#"
            SELECT
                id              AS "id: ProjectId",
                name,
                organization_id,
                tenant_id,
                created_at
            FROM projects
            WHERE id = $1
            "#,
            id as ProjectId,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}
