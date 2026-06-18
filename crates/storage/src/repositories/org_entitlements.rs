//! Story 24.1 — per-org billing entitlement repository.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EntitlementRow {
    pub org_id: Uuid,
    pub plan: String,
    pub seat_count: i32,
    pub stripe_customer_id: Option<String>,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct OrgEntitlementsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrgEntitlementsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn upsert(
        &self,
        org_id: Uuid,
        plan: &str,
        seat_count: u32,
        stripe_customer_id: Option<&str>,
        stripe_subscription_id: Option<&str>,
    ) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;

        sqlx::query(
            "INSERT INTO org_entitlements \
                (org_id, plan, seat_count, stripe_customer_id, stripe_subscription_id, synced_at) \
             VALUES ($1, $2, $3, $4, $5, now()) \
             ON CONFLICT (org_id) DO UPDATE \
             SET plan = EXCLUDED.plan, \
                 seat_count = EXCLUDED.seat_count, \
                 stripe_customer_id = EXCLUDED.stripe_customer_id, \
                 stripe_subscription_id = EXCLUDED.stripe_subscription_id, \
                 synced_at = now()",
        )
        .bind(org_id)
        .bind(plan)
        .bind(i32::try_from(seat_count).unwrap_or(i32::MAX))
        .bind(stripe_customer_id)
        .bind(stripe_subscription_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(&self, org_id: Uuid) -> Result<Option<EntitlementRow>, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;

        let row = sqlx::query_as::<_, EntitlementRow>(
            "SELECT org_id, plan, seat_count, stripe_customer_id, synced_at \
             FROM org_entitlements \
             WHERE org_id = $1",
        )
        .bind(org_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Count prompt runs created today (UTC) for this org. Used for per-org
    /// daily run cap enforcement (story 24.3, [p4-cap-1]).
    pub async fn count_org_runs_today(&self, org_id: Uuid) -> Result<u64, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM prompt_runs \
             WHERE org_id = $1 \
               AND created_at >= date_trunc('day', now() AT TIME ZONE 'UTC')",
        )
        .bind(org_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(u64::try_from(row.0).unwrap_or(u64::MAX))
    }

    /// Count active members (rows in operator_org_roles) for an org.
    pub async fn count_active_members(&self, org_id: Uuid) -> Result<u32, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM operator_org_roles WHERE org_id = $1")
                .bind(org_id)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(u32::try_from(row.0).unwrap_or(u32::MAX))
    }

    /// Count distinct brands (project_ids) granted within an org.
    pub async fn count_active_brands(&self, org_id: Uuid) -> Result<u32, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(DISTINCT project_id) FROM brand_grants WHERE org_id = $1")
                .bind(org_id)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(u32::try_from(row.0).unwrap_or(u32::MAX))
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
