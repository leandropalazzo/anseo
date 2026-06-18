//! Story 25.1 — org branding repository.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrgBrandingRow {
    pub org_id: Uuid,
    pub logo_url: Option<String>,
    pub accent_hex: Option<String>,
    pub updated_at: DateTime<Utc>,
}

pub struct OrgBrandingRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrgBrandingRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, org_id: Uuid) -> Result<Option<OrgBrandingRow>, Error> {
        // Use a transaction with org GUC set (RLS requirement)
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row = sqlx::query_as::<_, OrgBrandingRow>(
            "SELECT org_id, logo_url, accent_hex, updated_at FROM org_branding WHERE org_id = $1",
        )
        .bind(org_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn upsert(
        &self,
        org_id: Uuid,
        logo_url: Option<&str>,
        accent_hex: Option<&str>,
    ) -> Result<OrgBrandingRow, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row = sqlx::query_as::<_, OrgBrandingRow>(
            "INSERT INTO org_branding (org_id, logo_url, accent_hex, updated_at)
             VALUES ($1, $2, $3, now())
             ON CONFLICT (org_id) DO UPDATE
             SET logo_url = EXCLUDED.logo_url,
                 accent_hex = EXCLUDED.accent_hex,
                 updated_at = now()
             RETURNING org_id, logo_url, accent_hex, updated_at",
        )
        .bind(org_id)
        .bind(logo_url)
        .bind(accent_hex)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }
}

async fn set_org_guc(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: Uuid,
) -> Result<(), Error> {
    sqlx::query("SELECT set_config('app.org', $1, true)")
        .bind(org_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}
