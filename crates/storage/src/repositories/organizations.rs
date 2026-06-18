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

    /// Story 25.3 — grant portal access to a brand.
    ///
    /// Sets is_portal=true, replacing any prior portal grant for this operator
    /// (one portal brand per operator — enforced by partial unique index).
    /// The operator must already hold the Viewer role in the org; role management
    /// is the caller's responsibility.
    pub async fn grant_portal_brand(
        &self,
        org_id: Uuid,
        operator_id: Uuid,
        project_id: ProjectId,
        granted_by: Option<Uuid>,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        // Clear any existing portal grant for this operator first.
        sqlx::query("DELETE FROM brand_grants WHERE operator_id = $1 AND is_portal = true")
            .bind(operator_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO brand_grants \
             (operator_id, project_id, org_id, granted_by, is_portal) \
             VALUES ($1, $2, $3, $4, true) \
             ON CONFLICT (operator_id, project_id) DO UPDATE \
             SET org_id = EXCLUDED.org_id, \
                 granted_by = EXCLUDED.granted_by, \
                 granted_at = now(), \
                 is_portal = true",
        )
        .bind(operator_id)
        .bind(project_id)
        .bind(org_id)
        .bind(granted_by)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Story 25.3 — revoke a portal brand grant.
    ///
    /// Deletes the is_portal grant row. Returns true when a row was removed.
    /// Revocation takes effect immediately on the next request (no cache).
    pub async fn revoke_portal_brand(
        &self,
        operator_id: Uuid,
        project_id: ProjectId,
    ) -> Result<bool, Error> {
        let result = sqlx::query(
            "DELETE FROM brand_grants \
             WHERE operator_id = $1 AND project_id = $2 AND is_portal = true",
        )
        .bind(operator_id)
        .bind(project_id)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Story 27.1 — create an operator with email/password credentials.
    ///
    /// Returns the new operator's UUID. `login` is typically set to `email` for
    /// the email/password signup path. `password_hash` is stored as SHA-256 hex
    /// in this mock phase; production uses Argon2id (Story 27.2).
    pub async fn create_operator(
        &self,
        login: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<Uuid, Error> {
        let (id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO operators (login, email, password_hash) \
             VALUES ($1, $2, $3) \
             RETURNING id",
        )
        .bind(login)
        .bind(email)
        .bind(password_hash)
        .fetch_one(self.pool)
        .await?;
        Ok(id)
    }

    /// Story 27.1 — add an operator to an org with the given role.
    ///
    /// `role` must be a valid `org_role` enum value (owner/admin/operator/viewer/billing).
    /// Uses ON CONFLICT DO NOTHING so this is idempotent.
    pub async fn add_member(
        &self,
        org_id: Uuid,
        operator_id: Uuid,
        role: &str,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO operator_org_roles (operator_id, org_id, role) \
             VALUES ($1, $2, $3::org_role) \
             ON CONFLICT (operator_id, org_id) DO NOTHING",
        )
        .bind(operator_id)
        .bind(org_id)
        .bind(role)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Story 27.1 — set the org lifecycle state.
    ///
    /// `state` must be a valid `org_state` enum value (unconfigured/trial/active).
    pub async fn set_state(&self, org_id: Uuid, state: &str) -> Result<(), Error> {
        sqlx::query(
            "UPDATE organizations SET state = $1::org_state, updated_at = now() \
             WHERE id = $2",
        )
        .bind(state)
        .bind(org_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Story 25.3 — returns the portal brand (project_id) for an operator, if any.
    ///
    /// Used to verify portal scoping: if is_portal=true, the operator may only
    /// access this single brand. Absence means no portal grant.
    pub async fn portal_brand_for(&self, operator_id: Uuid) -> Result<Option<ProjectId>, Error> {
        let row: Option<(ProjectId,)> = sqlx::query_as::<_, (ProjectId,)>(
            "SELECT project_id FROM brand_grants \
             WHERE operator_id = $1 AND is_portal = true \
             LIMIT 1",
        )
        .bind(operator_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|(id,)| id))
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
