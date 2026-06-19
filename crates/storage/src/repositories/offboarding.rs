//! Story 27.10 — Org offboarding lifecycle repository.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

pub struct OffboardingRecord {
    pub id: Uuid,
    pub org_id: Uuid,
    pub state: String,
    pub stripe_subscription_id: Option<String>,
    pub stripe_customer_id: Option<String>,
    pub legal_hold: bool,
    pub export_grace_ends_at: DateTime<Utc>,
    pub shred_scheduled_at: Option<DateTime<Utc>>,
    pub shredded_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

type OffboardingRow = (
    Uuid,
    Uuid,
    String,
    Option<String>,
    Option<String>,
    bool,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    DateTime<Utc>,
);

fn row_to_record(r: OffboardingRow) -> OffboardingRecord {
    OffboardingRecord {
        id: r.0,
        org_id: r.1,
        state: r.2,
        stripe_subscription_id: r.3,
        stripe_customer_id: r.4,
        legal_hold: r.5,
        export_grace_ends_at: r.6,
        shred_scheduled_at: r.7,
        shredded_at: r.8,
        completed_at: r.9,
        created_at: r.10,
    }
}

pub struct OffboardingRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OffboardingRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Initiate offboarding — creates the record in `export_grace` state.
    /// `export_grace_ends_at` defaults to 30 days from now if not provided.
    pub async fn initiate(
        &self,
        org_id: Uuid,
        stripe_subscription_id: Option<&str>,
        stripe_customer_id: Option<&str>,
        export_grace_ends_at: DateTime<Utc>,
        initiated_by: Option<Uuid>,
    ) -> Result<OffboardingRecord, sqlx::Error> {
        let row: OffboardingRow = sqlx::query_as(
            r#"
            INSERT INTO org_offboarding
                (org_id, stripe_subscription_id, stripe_customer_id,
                 export_grace_ends_at, initiated_by)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (org_id) DO UPDATE
                SET state = EXCLUDED.state,
                    export_grace_ends_at = EXCLUDED.export_grace_ends_at,
                    updated_at = now()
            RETURNING id, org_id, state::text,
                      stripe_subscription_id, stripe_customer_id,
                      legal_hold, export_grace_ends_at,
                      shred_scheduled_at, shredded_at, completed_at, created_at
            "#,
        )
        .bind(org_id)
        .bind(stripe_subscription_id)
        .bind(stripe_customer_id)
        .bind(export_grace_ends_at)
        .bind(initiated_by)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_record(row))
    }

    pub async fn get_for_org(
        &self,
        org_id: Uuid,
    ) -> Result<Option<OffboardingRecord>, sqlx::Error> {
        let row: Option<OffboardingRow> = sqlx::query_as(
            r#"
            SELECT id, org_id, state::text,
                   stripe_subscription_id, stripe_customer_id,
                   legal_hold, export_grace_ends_at,
                   shred_scheduled_at, shredded_at, completed_at, created_at
            FROM org_offboarding
            WHERE org_id = $1
            "#,
        )
        .bind(org_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(row_to_record))
    }

    /// Advance state to `pending_shred` after the export grace window.
    /// Skipped if `legal_hold = true` (AC-1 legal-hold exception).
    pub async fn advance_to_pending_shred(
        &self,
        org_id: Uuid,
        shred_scheduled_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE org_offboarding
            SET state = 'pending_shred',
                shred_scheduled_at = $2,
                updated_at = now()
            WHERE org_id = $1
              AND state = 'export_grace'
              AND export_grace_ends_at <= now()
              AND legal_hold = false
            "#,
        )
        .bind(org_id)
        .bind(shred_scheduled_at)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Record that crypto-shred (CMK deletion) has been executed.
    pub async fn record_shredded(&self, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE org_offboarding
            SET state = 'shredded',
                shredded_at = now(),
                updated_at = now()
            WHERE org_id = $1 AND state = 'pending_shred'
            "#,
        )
        .bind(org_id)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Mark offboarding complete (Stripe teardown confirmed, no orphaned data).
    pub async fn complete(&self, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE org_offboarding
            SET state = 'complete',
                completed_at = now(),
                updated_at = now()
            WHERE org_id = $1 AND state = 'shredded'
            "#,
        )
        .bind(org_id)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
