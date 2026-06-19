//! Story 25.1 — org branding GET/PUT repository.
//! Story 25.2 — custom domain state machine methods added.

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
    pub custom_domain: Option<String>,
    pub domain_status: String,
    pub domain_txt_record: Option<String>,
    pub domain_verified_at: Option<DateTime<Utc>>,
    pub tls_status: String,
}

pub struct OrgBrandingRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrgBrandingRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, org_id: Uuid) -> Result<Option<OrgBrandingRow>, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row = sqlx::query_as::<_, OrgBrandingRow>(
            "SELECT org_id, logo_url, accent_hex, updated_at,
                    custom_domain, domain_status, domain_txt_record, domain_verified_at, tls_status
             FROM org_branding WHERE org_id = $1",
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
             RETURNING org_id, logo_url, accent_hex, updated_at,
                       custom_domain, domain_status, domain_txt_record, domain_verified_at, tls_status",
        )
        .bind(org_id)
        .bind(logo_url)
        .bind(accent_hex)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Story 25.2 — set custom_domain and move to pending_verification.
    pub async fn set_custom_domain(
        &self,
        org_id: Uuid,
        custom_domain: &str,
        txt_record: &str,
    ) -> Result<OrgBrandingRow, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row = sqlx::query_as::<_, OrgBrandingRow>(
            "INSERT INTO org_branding (org_id, custom_domain, domain_status, domain_txt_record, updated_at)
             VALUES ($1, $2, 'pending_verification', $3, now())
             ON CONFLICT (org_id) DO UPDATE
             SET custom_domain     = EXCLUDED.custom_domain,
                 domain_status     = 'pending_verification',
                 domain_txt_record = EXCLUDED.domain_txt_record,
                 domain_verified_at = NULL,
                 tls_status        = 'none',
                 updated_at        = now()
             RETURNING org_id, logo_url, accent_hex, updated_at,
                       custom_domain, domain_status, domain_txt_record, domain_verified_at, tls_status",
        )
        .bind(org_id)
        .bind(custom_domain)
        .bind(txt_record)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Story 25.2 — mark domain verified and move to provisioning.
    pub async fn verify_domain(&self, org_id: Uuid) -> Result<OrgBrandingRow, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row = sqlx::query_as::<_, OrgBrandingRow>(
            "UPDATE org_branding
             SET domain_status     = 'provisioning',
                 domain_verified_at = now(),
                 tls_status        = 'provisioning',
                 updated_at        = now()
             WHERE org_id = $1
             RETURNING org_id, logo_url, accent_hex, updated_at,
                       custom_domain, domain_status, domain_txt_record, domain_verified_at, tls_status",
        )
        .bind(org_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Story 25.2 — get domain status fields only (still full row for simplicity).
    pub async fn get_domain_status(&self, org_id: Uuid) -> Result<Option<OrgBrandingRow>, Error> {
        self.get(org_id).await
    }

    /// Story 25.2 — reset domain back to unclaimed.
    pub async fn clear_custom_domain(&self, org_id: Uuid) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        sqlx::query(
            "UPDATE org_branding
             SET custom_domain     = NULL,
                 domain_status     = 'unclaimed',
                 domain_txt_record = NULL,
                 domain_verified_at = NULL,
                 tls_status        = 'none',
                 updated_at        = now()
             WHERE org_id = $1",
        )
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
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
