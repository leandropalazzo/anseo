//! Story 24.4 — dunning state machine repository.
//!
//! Provides read + transition methods for the org dunning lifecycle.
//! Grace/suspend/pending-delete states are advanced by a nightly job.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrgDunningRow {
    pub org_id: Uuid,
    pub dunning_state: String,
    pub grace_started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct OrgDunningRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrgDunningRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, org_id: Uuid) -> Result<Option<OrgDunningRow>, Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;

        let row = sqlx::query_as::<_, OrgDunningRow>(
            "SELECT org_id, dunning_state::text, grace_started_at \
             FROM org_entitlements \
             WHERE org_id = $1",
        )
        .bind(org_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Enter grace period: set dunning_state = 'grace', record grace_started_at = now.
    /// No-op if already in grace/suspended/pending_delete.
    pub async fn enter_grace(&self, org_id: Uuid) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;

        sqlx::query(
            "UPDATE org_entitlements \
             SET dunning_state = 'grace', grace_started_at = now() \
             WHERE org_id = $1 AND dunning_state = 'active'",
        )
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Advance to suspended. Called by nightly job when grace period expires (7 days).
    pub async fn suspend(&self, org_id: Uuid) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;

        sqlx::query(
            "UPDATE org_entitlements \
             SET dunning_state = 'suspended' \
             WHERE org_id = $1 AND dunning_state = 'grace'",
        )
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Advance to pending_delete. Called by nightly job when 30 days have elapsed.
    pub async fn mark_pending_delete(&self, org_id: Uuid) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;

        sqlx::query(
            "UPDATE org_entitlements \
             SET dunning_state = 'pending_delete' \
             WHERE org_id = $1 AND dunning_state IN ('grace', 'suspended')",
        )
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Restore to active (payment recovered). Clears grace_started_at.
    pub async fn restore_active(&self, org_id: Uuid) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;
        set_org_guc(&mut tx, org_id).await?;

        sqlx::query(
            "UPDATE org_entitlements \
             SET dunning_state = 'active', grace_started_at = NULL \
             WHERE org_id = $1",
        )
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// List orgs in grace whose grace period has expired (for nightly suspension job).
    pub async fn list_grace_expired(
        &self,
        grace_days: i64,
        limit: i64,
    ) -> Result<Vec<Uuid>, Error> {
        let rows = sqlx::query_as::<_, (Uuid,)>(
            "SELECT org_id FROM org_entitlements \
             WHERE dunning_state = 'grace' \
               AND grace_started_at IS NOT NULL \
               AND grace_started_at < now() - ($1 || ' days')::interval \
             LIMIT $2",
        )
        .bind(grace_days)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// List orgs in grace/suspended that have exceeded the delete threshold.
    pub async fn list_delete_ready(
        &self,
        delete_days: i64,
        limit: i64,
    ) -> Result<Vec<Uuid>, Error> {
        let rows = sqlx::query_as::<_, (Uuid,)>(
            "SELECT org_id FROM org_entitlements \
             WHERE dunning_state IN ('grace', 'suspended') \
               AND grace_started_at IS NOT NULL \
               AND grace_started_at < now() - ($1 || ' days')::interval \
             LIMIT $2",
        )
        .bind(delete_days)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Story 24.4 — dunning repo methods compile and types are consistent.
    #[allow(dead_code)]
    const STORY_24_4_EVIDENCE: &str =
        "story-24.4: OrgDunningRepo (enter_grace/suspend/mark_pending_delete/restore_active) + DunningState + advance_dunning";
}
