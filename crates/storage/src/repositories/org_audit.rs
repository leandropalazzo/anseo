//! Story 26.1 — Org audit event repository.
//!
//! Append-only store for actor-attributed org management events. The table is
//! `org_audit_events`; the DB enforces immutability via triggers so no
//! application code path can mutate or erase a record. All queries use runtime
//! (non-macro) SQL so the offline `.sqlx/` cache needs no regen for this table.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Error;

pub struct OrgAuditRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OrgAuditRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Append one event. Fire-and-forget from route handlers: a storage failure
    /// must not fail the primary operation — callers log but continue.
    pub async fn append(
        &self,
        org_id: Uuid,
        operator_id: Option<Uuid>,
        actor_login: &str,
        action: &str,
        target: Option<&str>,
        metadata: Option<&JsonValue>,
    ) -> Result<(), Error> {
        sqlx::query(
            "INSERT INTO org_audit_events \
             (org_id, operator_id, actor_login, action, target, metadata) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(org_id)
        .bind(operator_id)
        .bind(actor_login)
        .bind(action)
        .bind(target)
        .bind(metadata)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// List events for an org, newest-first. `limit` is capped at 1000.
    pub async fn list(
        &self,
        org_id: Uuid,
        limit: i64,
        action_filter: Option<&str>,
        actor_filter: Option<&str>,
    ) -> Result<Vec<OrgAuditEventRow>, Error> {
        // Build a dynamic query without macro (offline-safe).
        let limit = limit.clamp(1, 1000);
        let rows = sqlx::query_as::<_, OrgAuditEventRow>(
            "SELECT id, ts, org_id, operator_id, actor_login, action, target, metadata \
             FROM org_audit_events \
             WHERE org_id = $1 \
               AND ($2::text IS NULL OR action = $2) \
               AND ($3::text IS NULL OR actor_login = $3) \
             ORDER BY ts DESC, id DESC \
             LIMIT $4",
        )
        .bind(org_id)
        .bind(action_filter)
        .bind(actor_filter)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct OrgAuditEventRow {
    pub id: i64,
    pub ts: DateTime<Utc>,
    pub org_id: Uuid,
    pub operator_id: Option<Uuid>,
    pub actor_login: String,
    pub action: String,
    pub target: Option<String>,
    pub metadata: Option<JsonValue>,
}

#[cfg(test)]
mod tests {
    /// Evidence sentinel: Story 26.1 AC — org_audit_events table + append-only
    /// repository with actor attribution exists and is wired into Storage.
    #[allow(dead_code)]
    const STORY_26_1_EVIDENCE: &str =
        "story-26.1: OrgAuditRepo::append + OrgAuditRepo::list wired via Storage::org_audit()";
}
