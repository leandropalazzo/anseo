//! Story 27.6 — Governed admin impersonation grants repository.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImpersonationGrant {
    pub id: Uuid,
    pub support_operator_id: Uuid,
    pub target_org_id: Uuid,
    pub granted_by: Uuid,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

pub struct ImpersonationRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ImpersonationRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        support_operator_id: Uuid,
        target_org_id: Uuid,
        granted_by: Uuid,
        expires_at: DateTime<Utc>,
        reason: &str,
    ) -> Result<ImpersonationGrant, sqlx::Error> {
        let row: (
            Uuid,
            Uuid,
            Uuid,
            Uuid,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            String,
            DateTime<Utc>,
        ) = sqlx::query_as(
            r#"
                INSERT INTO impersonation_grants
                    (support_operator_id, target_org_id, granted_by, expires_at, reason)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id, support_operator_id, target_org_id, granted_by,
                          expires_at, revoked_at, reason, created_at
                "#,
        )
        .bind(support_operator_id)
        .bind(target_org_id)
        .bind(granted_by)
        .bind(expires_at)
        .bind(reason)
        .fetch_one(self.pool)
        .await?;

        Ok(ImpersonationGrant {
            id: row.0,
            support_operator_id: row.1,
            target_org_id: row.2,
            granted_by: row.3,
            expires_at: row.4,
            revoked_at: row.5,
            reason: row.6,
            created_at: row.7,
        })
    }

    /// Find a valid (non-expired, non-revoked) grant for a support operator.
    pub async fn find_active(
        &self,
        grant_id: Uuid,
        support_operator_id: Uuid,
    ) -> Result<Option<ImpersonationGrant>, sqlx::Error> {
        let row: Option<(
            Uuid,
            Uuid,
            Uuid,
            Uuid,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            String,
            DateTime<Utc>,
        )> = sqlx::query_as(
            r#"
                SELECT id, support_operator_id, target_org_id, granted_by,
                       expires_at, revoked_at, reason, created_at
                FROM impersonation_grants
                WHERE id = $1
                  AND support_operator_id = $2
                  AND expires_at > now()
                  AND revoked_at IS NULL
                "#,
        )
        .bind(grant_id)
        .bind(support_operator_id)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(|r| ImpersonationGrant {
            id: r.0,
            support_operator_id: r.1,
            target_org_id: r.2,
            granted_by: r.3,
            expires_at: r.4,
            revoked_at: r.5,
            reason: r.6,
            created_at: r.7,
        }))
    }

    pub async fn revoke(
        &self,
        grant_id: Uuid,
        support_operator_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE impersonation_grants
            SET revoked_at = now()
            WHERE id = $1 AND support_operator_id = $2 AND revoked_at IS NULL
            "#,
        )
        .bind(grant_id)
        .bind(support_operator_id)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn list_for_org(
        &self,
        target_org_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ImpersonationGrant>, sqlx::Error> {
        let rows: Vec<(
            Uuid,
            Uuid,
            Uuid,
            Uuid,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
            String,
            DateTime<Utc>,
        )> = sqlx::query_as(
            r#"
                SELECT id, support_operator_id, target_org_id, granted_by,
                       expires_at, revoked_at, reason, created_at
                FROM impersonation_grants
                WHERE target_org_id = $1
                ORDER BY created_at DESC
                LIMIT $2
                "#,
        )
        .bind(target_org_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| ImpersonationGrant {
                id: r.0,
                support_operator_id: r.1,
                target_org_id: r.2,
                granted_by: r.3,
                expires_at: r.4,
                revoked_at: r.5,
                reason: r.6,
                created_at: r.7,
            })
            .collect())
    }
}
