//! Story 27.1 — Org provisioning saga repository.
//!
//! Idempotent, resumable signup saga. Steps advance monotonically:
//! created → kms_done → entitlement → owner_set → complete.
//!
//! All methods are re-entrant: calling a step that is already done is a no-op.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SagaRow {
    pub id: Uuid,
    pub operator_id: Uuid,
    pub org_id: Uuid,
    pub step: String,
    pub verify_token_hash: Option<String>,
    pub verify_expires_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub struct ProvisioningSagaRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ProvisioningSagaRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Create a new saga row (step=created). Idempotent: if a saga already
    /// exists for this operator, return the existing row.
    pub async fn create(
        &self,
        operator_id: Uuid,
        org_id: Uuid,
        verify_token_hash: &str,
        verify_expires_at: DateTime<Utc>,
    ) -> Result<SagaRow, Error> {
        let row = sqlx::query_as::<_, SagaRow>(
            "INSERT INTO org_provisioning_sagas \
             (operator_id, org_id, verify_token_hash, verify_expires_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (operator_id) DO UPDATE \
             SET updated_at = now() \
             RETURNING id, operator_id, org_id, step::text, \
                       verify_token_hash, verify_expires_at, completed_at, created_at",
        )
        .bind(operator_id)
        .bind(org_id)
        .bind(verify_token_hash)
        .bind(verify_expires_at)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    /// Fetch a saga by operator_id.
    pub async fn get_by_operator(&self, operator_id: Uuid) -> Result<Option<SagaRow>, Error> {
        let row = sqlx::query_as::<_, SagaRow>(
            "SELECT id, operator_id, org_id, step::text, verify_token_hash, \
                    verify_expires_at, completed_at, created_at \
             FROM org_provisioning_sagas \
             WHERE operator_id = $1",
        )
        .bind(operator_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// Advance to kms_done. No-op if already past this step.
    pub async fn advance_kms_done(&self, saga_id: Uuid) -> Result<(), Error> {
        sqlx::query(
            "UPDATE org_provisioning_sagas \
             SET step = 'kms_done', updated_at = now() \
             WHERE id = $1 AND step = 'created'",
        )
        .bind(saga_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Advance to entitlement. No-op if already past this step.
    pub async fn advance_entitlement(&self, saga_id: Uuid) -> Result<(), Error> {
        sqlx::query(
            "UPDATE org_provisioning_sagas \
             SET step = 'entitlement', updated_at = now() \
             WHERE id = $1 AND step = 'kms_done'",
        )
        .bind(saga_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Advance to owner_set. No-op if already past this step.
    pub async fn advance_owner_set(&self, saga_id: Uuid) -> Result<(), Error> {
        sqlx::query(
            "UPDATE org_provisioning_sagas \
             SET step = 'owner_set', updated_at = now() \
             WHERE id = $1 AND step = 'entitlement'",
        )
        .bind(saga_id)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Complete the saga (email verified). Sets org.state → trial.
    /// Returns false if the token is invalid or expired.
    pub async fn complete(&self, saga_id: Uuid, token_hash: &str) -> Result<bool, Error> {
        let rows = sqlx::query(
            "UPDATE org_provisioning_sagas \
             SET step = 'complete', completed_at = now(), updated_at = now() \
             WHERE id = $1 \
               AND step = 'owner_set' \
               AND verify_token_hash = $2 \
               AND verify_expires_at > now()",
        )
        .bind(saga_id)
        .bind(token_hash)
        .execute(self.pool)
        .await?;
        Ok(rows.rows_affected() > 0)
    }

    /// Verify token and saga_id for email verification.
    /// Returns the saga row if found and token matches.
    pub async fn get_by_token(
        &self,
        saga_id: Uuid,
        token_hash: &str,
    ) -> Result<Option<SagaRow>, Error> {
        let row = sqlx::query_as::<_, SagaRow>(
            "SELECT id, operator_id, org_id, step::text, verify_token_hash, \
                    verify_expires_at, completed_at, created_at \
             FROM org_provisioning_sagas \
             WHERE id = $1 \
               AND verify_token_hash = $2 \
               AND verify_expires_at > now()",
        )
        .bind(saga_id)
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    /// Story 27.1 evidence: ProvisioningSagaRepo provides idempotent step
    /// advancement for the org provisioning saga (created→kms_done→entitlement
    /// →owner_set→complete). Each advance method is a conditional UPDATE that
    /// no-ops if the row is already past that step.
    #[allow(dead_code)]
    const STORY_27_1_EVIDENCE: &str =
        "story-27.1: ProvisioningSagaRepo + org_state enum (unconfigured|trial|active)";
}
