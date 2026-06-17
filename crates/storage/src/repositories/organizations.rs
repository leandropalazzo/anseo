//! Story 20.8 — Organizations CRUD substrate.
//!
//! Provides basic CRUD operations for the `organizations` table. This is
//! the storage substrate for the `/v1/orgs` API surface (Epic 20 Phase 4).
//! Auth/RBAC enforcement is added in subsequent stories (21.1, 22.x).

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
             WHERE org_id = $1 \
             ORDER BY created_at ASC",
        )
        .bind(org_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
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
