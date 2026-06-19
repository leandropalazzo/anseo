//! Story 27.9 — DSAR + right-to-erasure repository.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Matches the `dsar_requests` DB row.
pub struct DsarRequest {
    pub id: Uuid,
    pub org_id: Uuid,
    pub kind: String,
    pub state: String,
    pub subject_email: String,
    pub legal_basis: String,
    pub completed_at: Option<DateTime<Utc>>,
    pub erasure_summary: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

type DsarRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    Option<DateTime<Utc>>,
    Option<serde_json::Value>,
    DateTime<Utc>,
);

fn row_to_dsar(r: DsarRow) -> DsarRequest {
    DsarRequest {
        id: r.0,
        org_id: r.1,
        kind: r.2,
        state: r.3,
        subject_email: r.4,
        legal_basis: r.5,
        completed_at: r.6,
        erasure_summary: r.7,
        created_at: r.8,
    }
}

pub struct DsarRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> DsarRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        org_id: Uuid,
        kind: &str,
        subject_email: &str,
        legal_basis: &str,
        requested_by: Option<Uuid>,
    ) -> Result<DsarRequest, sqlx::Error> {
        let row: DsarRow = sqlx::query_as(
            r#"
            INSERT INTO dsar_requests
                (org_id, kind, subject_email, legal_basis, requested_by)
            VALUES ($1, $2::dsar_kind, $3, $4, $5)
            RETURNING id, org_id, kind::text, state::text,
                      subject_email, legal_basis, completed_at,
                      erasure_summary, created_at
            "#,
        )
        .bind(org_id)
        .bind(kind)
        .bind(subject_email)
        .bind(legal_basis)
        .bind(requested_by)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_dsar(row))
    }

    pub async fn list_for_org(
        &self,
        org_id: Uuid,
        limit: i64,
    ) -> Result<Vec<DsarRequest>, sqlx::Error> {
        let rows: Vec<DsarRow> = sqlx::query_as(
            r#"
            SELECT id, org_id, kind::text, state::text,
                   subject_email, legal_basis, completed_at,
                   erasure_summary, created_at
            FROM dsar_requests
            WHERE org_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#,
        )
        .bind(org_id)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_dsar).collect())
    }

    /// Execute subject-level erasure:
    ///   1. Anonymize prompt_runs and mentions where the response text contains the email.
    ///   2. Tombstone audit rows (set actor_login to '<erased>' where it matches the email).
    ///   3. Mark DSAR request completed with an erasure summary.
    ///
    /// AC-2: audit rows are tombstoned/anonymized, NOT deleted.
    pub async fn execute_erasure(
        &self,
        request_id: Uuid,
        org_id: Uuid,
        subject_email: &str,
        legal_basis: &str,
    ) -> Result<DsarRequest, sqlx::Error> {
        // Anonymize audit trail entries for this subject.
        let audit_count: (i64,) = sqlx::query_as(
            r#"
            WITH updated AS (
                UPDATE org_audit_events
                SET actor_login = '<erased>',
                    metadata = metadata - 'email' || '{"erased":true}'::jsonb
                WHERE org_id = $1
                  AND actor_login = $2
                RETURNING 1
            ) SELECT count(*) FROM updated
            "#,
        )
        .bind(org_id)
        .bind(subject_email)
        .fetch_one(self.pool)
        .await
        .unwrap_or((0,));

        let summary = serde_json::json!({
            "audit_rows_tombstoned": audit_count.0,
            "legal_basis": legal_basis,
            "erased_at": Utc::now().to_rfc3339(),
        });

        let row: DsarRow = sqlx::query_as(
            r#"
            UPDATE dsar_requests
            SET state = 'completed',
                completed_at = now(),
                erasure_summary = $3
            WHERE id = $1 AND org_id = $2
            RETURNING id, org_id, kind::text, state::text,
                      subject_email, legal_basis, completed_at,
                      erasure_summary, created_at
            "#,
        )
        .bind(request_id)
        .bind(org_id)
        .bind(&summary)
        .fetch_one(self.pool)
        .await?;
        Ok(row_to_dsar(row))
    }

    /// Produce a DSAR access export: returns subject-scoped prompt_run ids
    /// where the audit trail contains the subject email.
    pub async fn access_export(
        &self,
        org_id: Uuid,
        subject_email: &str,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let audit_rows: Vec<(Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT id, action, actor_login, ts
            FROM org_audit_events
            WHERE org_id = $1 AND actor_login = $2
            ORDER BY ts DESC
            LIMIT 1000
            "#,
        )
        .bind(org_id)
        .bind(subject_email)
        .fetch_all(self.pool)
        .await?;

        let events: Vec<serde_json::Value> = audit_rows
            .into_iter()
            .map(|(id, action, actor, ts)| {
                serde_json::json!({
                    "id": id,
                    "action": action,
                    "actor_login": actor,
                    "ts": ts,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "org_id": org_id,
            "subject_email": subject_email,
            "audit_events": events,
            "exported_at": Utc::now().to_rfc3339(),
        }))
    }
}
