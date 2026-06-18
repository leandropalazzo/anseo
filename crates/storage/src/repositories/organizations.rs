//! Story 20.8 — Organizations CRUD substrate.
//!
//! Provides basic CRUD operations for the `organizations` table. This is
//! the storage substrate for the `/v1/orgs` API surface (Epic 20 Phase 4).
//! Auth/RBAC enforcement is added in subsequent stories (21.1, 22.x).

use anseo_core::ids::ProjectId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrgRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub region: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct OrgsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrgsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<OrgRow>, Error> {
        let rows = sqlx::query_as::<_, OrgRow>(
            "SELECT id, slug, name, region, created_at, updated_at \
             FROM organizations \
             ORDER BY created_at ASC",
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<OrgRow>, Error> {
        let row = sqlx::query_as::<_, OrgRow>(
            "SELECT id, slug, name, region, created_at, updated_at \
             FROM organizations \
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn get_by_slug(&self, slug: &str) -> Result<Option<OrgRow>, Error> {
        let row = sqlx::query_as::<_, OrgRow>(
            "SELECT id, slug, name, region, created_at, updated_at \
             FROM organizations \
             WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    pub async fn create(
        &self,
        slug: &str,
        name: &str,
        region: Option<&str>,
    ) -> Result<OrgRow, Error> {
        let row = sqlx::query_as::<_, OrgRow>(
            "INSERT INTO organizations (slug, name, region) \
             VALUES ($1, $2, $3) \
             RETURNING id, slug, name, region, created_at, updated_at",
        )
        .bind(slug)
        .bind(name)
        .bind(region)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    /// List all projects (brands) belonging to a given org.
    /// Uses the `brands` VIEW which exposes `brand_id` as an alias for `id`.
    pub async fn list_brands(&self, org_id: Uuid) -> Result<Vec<OrgBrandRow>, Error> {
        let rows = sqlx::query_as::<_, OrgBrandRow>(
            "SELECT id, brand_id, name, site_url, created_at, archived_at \
             FROM brands \
             WHERE org_id = $1 AND archived_at IS NULL \
             ORDER BY created_at ASC",
        )
        .bind(org_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// True when an operator has a live grant for a brand in the org.
    ///
    /// Story 22.3: this is intentionally checked at request/list time, never
    /// cached at login, so revocations apply on the next request.
    pub async fn has_brand_grant(
        &self,
        org_id: Uuid,
        operator_id: Uuid,
        project_id: ProjectId,
    ) -> Result<bool, Error> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (\
                SELECT 1 FROM brand_grants \
                WHERE org_id = $1 AND operator_id = $2 AND project_id = $3\
             )",
        )
        .bind(org_id)
        .bind(operator_id)
        .bind(project_id)
        .fetch_one(self.pool)
        .await?;
        Ok(exists)
    }

    /// List only brands granted to the operator. Owner/Admin callers should use
    /// [`list_brands`] instead; this method is for Operator/Viewer scoping.
    pub async fn list_brands_granted_to(
        &self,
        org_id: Uuid,
        operator_id: Uuid,
    ) -> Result<Vec<OrgBrandRow>, Error> {
        let rows = sqlx::query_as::<_, OrgBrandRow>(
            "SELECT b.id, b.brand_id, b.name, b.site_url, b.created_at, b.archived_at \
             FROM brands b \
             JOIN brand_grants g \
               ON g.project_id = b.brand_id \
              AND g.org_id = b.org_id \
              AND g.operator_id = $2 \
             WHERE b.org_id = $1 AND b.archived_at IS NULL \
             ORDER BY b.created_at ASC",
        )
        .bind(org_id)
        .bind(operator_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Grant an operator access to a brand. Idempotent for repeated grants.
    pub async fn grant_brand(
        &self,
        org_id: Uuid,
        operator_id: Uuid,
        project_id: ProjectId,
        granted_by: Option<Uuid>,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO brand_grants (operator_id, project_id, org_id, granted_by) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (operator_id, project_id) DO UPDATE \
             SET org_id = EXCLUDED.org_id, \
                 granted_by = EXCLUDED.granted_by, \
                 granted_at = now()",
        )
        .bind(operator_id)
        .bind(project_id)
        .bind(org_id)
        .bind(granted_by)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Revoke an operator's brand access. Idempotent; returns true when a row
    /// was actually removed.
    pub async fn revoke_brand_grant(
        &self,
        operator_id: Uuid,
        project_id: ProjectId,
    ) -> Result<bool, Error> {
        let result =
            sqlx::query("DELETE FROM brand_grants WHERE operator_id = $1 AND project_id = $2")
                .bind(operator_id)
                .bind(project_id)
                .execute(self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OrgBrandRow {
    pub id: Uuid,
    pub brand_id: Uuid,
    pub name: String,
    pub site_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}
