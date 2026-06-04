//! Story 31-1 — Alert-rules repository.
//!
//! Runtime `sqlx::query_as` / `query` (no compile-time `query!` macros), to
//! match the workspace's no-offline-cache discipline for the newer tables.
//! Borrows `&PgPool` like the sibling repos (`PromptRepo`, `ProjectRepo`).

use sqlx::types::Json;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AlertRuleRecord {
    pub id: Uuid,
    pub name: String,
    /// Maps to the UI `on` condition-expression field.
    pub condition: String,
    pub target: String,
    pub channels: Json<Vec<String>>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct AlertRulesRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> AlertRulesRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    pub async fn list(&self) -> Result<Vec<AlertRuleRecord>, sqlx::Error> {
        sqlx::query_as::<_, AlertRuleRecord>(
            "SELECT id, name, condition, target, channels, status, created_at \
             FROM alert_rules ORDER BY created_at DESC",
        )
        .fetch_all(self.pool)
        .await
    }

    pub async fn create(
        &self,
        name: &str,
        condition: &str,
        target: &str,
        channels: &[String],
    ) -> Result<AlertRuleRecord, sqlx::Error> {
        // Project-scoped via the single configured project (Phase 2 is
        // single-project; mirrors how the schedules surface scopes writes).
        sqlx::query_as::<_, AlertRuleRecord>(
            "INSERT INTO alert_rules (project_id, name, condition, target, channels) \
             VALUES ((SELECT id FROM projects ORDER BY created_at LIMIT 1), $1, $2, $3, $4) \
             RETURNING id, name, condition, target, channels, status, created_at",
        )
        .bind(name)
        .bind(condition)
        .bind(target)
        .bind(Json(channels.to_vec()))
        .fetch_one(self.pool)
        .await
    }

    pub async fn set_status(
        &self,
        name: &str,
        status: &str,
    ) -> Result<Option<AlertRuleRecord>, sqlx::Error> {
        sqlx::query_as::<_, AlertRuleRecord>(
            "UPDATE alert_rules SET status = $2 WHERE name = $1 \
             RETURNING id, name, condition, target, channels, status, created_at",
        )
        .bind(name)
        .bind(status)
        .fetch_optional(self.pool)
        .await
    }

    pub async fn delete(&self, name: &str) -> Result<u64, sqlx::Error> {
        let res = sqlx::query("DELETE FROM alert_rules WHERE name = $1")
            .bind(name)
            .execute(self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}
