use anseo_core::ids::ProjectId;
use anseo_core::{project_id_for_name, BrandConfig};
use chrono::Utc;
use serde_json::Value as JsonValue;
use sqlx::PgPool;

use crate::error::Error;
use crate::models::{BrandRow, ProjectRow};

pub struct ProjectRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ProjectRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, row: &ProjectRow) -> Result<ProjectId, Error> {
        // `variants` / `competitors` are omitted here on purpose — they carry
        // DB defaults (`'{}'` / `'[]'`). Brand config is written via
        // [`update_brand`] / [`rename_on_empty`].
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

    /// Read the brand config (name + variants + competitors) for a project.
    pub async fn get_brand(&self, id: ProjectId) -> Result<Option<BrandRow>, Error> {
        let row = sqlx::query_as!(
            BrandRow,
            r#"
            SELECT
                id           AS "id: ProjectId",
                name,
                variants     AS "variants!: Vec<String>",
                competitors  AS "competitors!: JsonValue",
                site_url
            FROM projects
            WHERE id = $1
            "#,
            id as ProjectId,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// List every non-archived project ordered by creation (registry view).
    /// This is the multi-project entry point Epic 36 builds on — storage now
    /// permits any number of coexisting projects; archived rows are excluded.
    pub async fn list_projects(&self) -> Result<Vec<ProjectRow>, Error> {
        let rows = sqlx::query_as!(
            ProjectRow,
            r#"
            SELECT
                id              AS "id: ProjectId",
                name,
                organization_id,
                tenant_id,
                created_at
            FROM projects
            WHERE archived_at IS NULL
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Fetch a single project by id (registry alias of [`get`]). Returns `None`
    /// for an unknown id, including ids that were archived-then-purged.
    pub async fn get_project(&self, id: ProjectId) -> Result<Option<ProjectRow>, Error> {
        self.get(id).await
    }

    /// Create a project from a [`BrandConfig`], deriving its `project_id` from
    /// the brand name via the existing [`project_id_for_name`] derivation (the
    /// same identity the YAML boot path uses). `variants` / `site_url` are
    /// written through; `competitors` keeps its DB default and is edited later
    /// via [`update_brand`]. Returns the derived [`ProjectId`].
    pub async fn create_project(&self, brand: &BrandConfig) -> Result<ProjectId, Error> {
        let id = project_id_for_name(&brand.name);
        sqlx::query!(
            r#"
            INSERT INTO projects (id, name, variants, site_url, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            id as ProjectId,
            brand.name,
            &brand.variants,
            brand.site_url.as_deref(),
            Utc::now(),
        )
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Soft-delete a project: stamp `archived_at` so it drops out of
    /// [`list_projects`] while its rows (and FK-referenced children) are
    /// preserved. Idempotent — re-archiving an already-archived project leaves
    /// the original timestamp untouched.
    pub async fn archive_project(&self, id: ProjectId) -> Result<(), Error> {
        sqlx::query!(
            r#"
            UPDATE projects
            SET archived_at = now()
            WHERE id = $1 AND archived_at IS NULL
            "#,
            id as ProjectId,
        )
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Return the single project's brand config when the deployment holds
    /// exactly one project (the legacy single-project precedence fallback used
    /// by the API boot path: when exactly one project exists, its
    /// name/variants/competitors win over the bootstrap `anseo.yaml`).
    ///
    /// Storage no longer forbids multiple projects — this helper simply yields
    /// `None` when zero or more-than-one projects exist, leaving the boot path
    /// to fall back to the YAML. Archived projects are excluded so a sole
    /// *active* project still resolves after siblings are archived.
    ///
    /// **Story 36.11 (RISK-6)** — this is the storage-layer half of the
    /// v0.2.0 upgrade guarantee. A single-project deployment upgraded to the
    /// multi-project binary calls this on every header-less request; it yields
    /// the sole project so no manual steps or configuration changes are needed.
    pub async fn get_single_brand(&self) -> Result<Option<BrandRow>, Error> {
        let rows = sqlx::query_as!(
            BrandRow,
            r#"
            SELECT
                id           AS "id: ProjectId",
                name,
                variants     AS "variants!: Vec<String>",
                competitors  AS "competitors!: JsonValue",
                site_url
            FROM projects
            WHERE archived_at IS NULL
            ORDER BY created_at ASC
            LIMIT 2
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        if rows.len() == 1 {
            Ok(rows.into_iter().next())
        } else {
            Ok(None)
        }
    }

    /// Count `prompt_runs` belonging to a project (via its prompts). Used to
    /// gate a rename: re-keying with existing runs requires the full cascade
    /// re-key, which is deferred — so a rename is only permitted at zero runs.
    pub async fn prompt_run_count(&self, id: ProjectId) -> Result<i64, Error> {
        let n: i64 = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM prompt_runs pr
            JOIN prompts p ON p.id = pr.prompt_id
            WHERE p.project_id = $1
            "#,
            id as ProjectId,
        )
        .fetch_one(self.pool)
        .await?;
        Ok(n)
    }

    /// Update brand config in place WITHOUT changing identity. Used when the
    /// brand name is unchanged (only variants/competitors edited).
    pub async fn update_brand(
        &self,
        id: ProjectId,
        name: &str,
        variants: &[String],
        competitors: &JsonValue,
        site_url: Option<&str>,
    ) -> Result<(), Error> {
        sqlx::query!(
            r#"
            UPDATE projects
            SET name = $2, variants = $3, competitors = $4, site_url = $5
            WHERE id = $1
            "#,
            id as ProjectId,
            name,
            variants,
            competitors,
            site_url,
        )
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Re-key a project to a new id (brand rename) — only valid when the
    /// project has no `prompt_runs`. Re-derives every prompt id alongside the
    /// project id (prompt ids fold in the brand name). All moves happen in one
    /// transaction: insert the new project row, re-point every child table's
    /// `project_id`, re-key prompt ids, then delete the old project row.
    ///
    /// `prompt_id_remap` is `(old_prompt_id, new_prompt_id)` pairs computed by
    /// the caller from the new brand name.
    #[allow(clippy::too_many_arguments)]
    pub async fn rename_on_empty(
        &self,
        old_id: ProjectId,
        new_id: ProjectId,
        new_name: &str,
        variants: &[String],
        competitors: &JsonValue,
        site_url: Option<&str>,
        prompt_id_remap: &[(anseo_core::PromptId, anseo_core::PromptId)],
    ) -> Result<(), Error> {
        use anseo_core::PromptId;
        let mut tx = self.pool.begin().await?;

        // Carry over org/tenant/created_at from the existing row.
        let existing = sqlx::query!(
            r#"SELECT organization_id, tenant_id, created_at FROM projects WHERE id = $1"#,
            old_id as ProjectId,
        )
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO projects (id, name, variants, competitors, site_url, organization_id, tenant_id, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
            new_id as ProjectId,
            new_name,
            variants,
            competitors,
            site_url,
            existing.organization_id,
            existing.tenant_id,
            existing.created_at,
        )
        .execute(&mut *tx)
        .await?;

        // Re-point every child table that carries project_id. The new parent
        // row already exists, so these UPDATEs satisfy the FK; the old row is
        // deleted last once nothing references it. Dynamic SQL (not query!)
        // keeps this list maintainable without a macro per table.
        for table in [
            "prompts",
            "api_keys",
            "schedules",
            "webhooks",
            "notification_targets",
            "benchmark_consent",
            // Story 44.1: identified-tier contributions carry an ON DELETE
            // RESTRICT FK to projects, so the old project row cannot be deleted
            // (below) until these rows are re-pointed at the new id. Their
            // `consent_record_id` FK targets benchmark_consent.id (the row id,
            // unchanged by a re-key), so it stays valid across the move.
            "contributions",
            "etl_jobs",
            "alert_rules",
            // No FK to projects, but scoped by project_id (see migration
            // 20260530120000 / analytics_migration_state).
            "recommendations",
            "analytics_migration_state",
        ] {
            let sql = format!("UPDATE {table} SET project_id = $1 WHERE project_id = $2");
            sqlx::query(&sql)
                .bind(new_id)
                .bind(old_id)
                .execute(&mut *tx)
                .await?;
        }

        // Re-key prompt ids (safe: zero prompt_runs reference them here).
        for (old_pid, new_pid) in prompt_id_remap {
            sqlx::query!(
                r#"UPDATE prompts SET id = $1 WHERE id = $2"#,
                *new_pid as PromptId,
                *old_pid as PromptId,
            )
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query!(r#"DELETE FROM projects WHERE id = $1"#, old_id as ProjectId,)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
