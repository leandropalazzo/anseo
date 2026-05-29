//! Repository for the Phase 2 `webhooks` table (Story 12.4 — FR-35).
//!
//! Mirrors the table defined in migration 20260528120000:
//! `(id, project_id, name, target_url, secret_ciphertext, event_kinds,
//!   disabled, disabled_reason, organization_id, tenant_id, created_at)`.
//!
//! Runtime `sqlx::query` form (no `.sqlx/` offline cache for the Phase 2
//! migrations yet — matches the api_keys and webhook_deliveries repos).
//!
//! Secrets are stored as `secret_ciphertext` (TEXT) so the column shape
//! survives a future at-rest-encryption change. The caller is responsible
//! for encrypting before write and decrypting after read; the in-tree
//! `opengeo_core::secret_store` keychain backend is the Phase 2 default.

use chrono::{DateTime, Utc};
use opengeo_core::ids::ProjectId;
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct WebhookRow {
    pub id: Uuid,
    pub project_id: ProjectId,
    pub name: String,
    pub target_url: String,
    pub secret_ciphertext: String,
    pub event_kinds: JsonValue,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct WebhookRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> WebhookRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Persist a freshly-declared webhook. The caller has already
    /// encrypted the secret material (the column accepts ciphertext
    /// only). Duplicate `(project_id, name)` surfaces as a UNIQUE
    /// violation.
    pub async fn insert(
        &self,
        project_id: ProjectId,
        name: &str,
        target_url: &str,
        secret_ciphertext: &str,
        event_kinds: &JsonValue,
    ) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"
            INSERT INTO webhooks
                (id, project_id, name, target_url, secret_ciphertext, event_kinds)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(name)
        .bind(target_url)
        .bind(secret_ciphertext)
        .bind(event_kinds)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_by_id(&self, id: Uuid) -> Result<Option<WebhookRow>, Error> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, name, target_url, secret_ciphertext,
                   event_kinds, disabled, disabled_reason, created_at
            FROM webhooks
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        row.map(row_to_webhook).transpose()
    }

    pub async fn get_by_project_name(
        &self,
        project_id: ProjectId,
        name: &str,
    ) -> Result<Option<WebhookRow>, Error> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, name, target_url, secret_ciphertext,
                   event_kinds, disabled, disabled_reason, created_at
            FROM webhooks
            WHERE project_id = $1
              AND name = $2
            "#,
        )
        .bind(project_id)
        .bind(name)
        .fetch_optional(self.pool)
        .await?;
        row.map(row_to_webhook).transpose()
    }

    pub async fn list_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<WebhookRow>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, project_id, name, target_url, secret_ciphertext,
                   event_kinds, disabled, disabled_reason, created_at
            FROM webhooks
            WHERE project_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id)
        .fetch_all(self.pool)
        .await?;
        rows.into_iter().map(row_to_webhook).collect()
    }

    /// Disable a webhook by name. Idempotent: returns `Ok(false)` when no
    /// active row existed under that name (already disabled or never
    /// declared).
    pub async fn disable(
        &self,
        project_id: ProjectId,
        name: &str,
        reason: &str,
    ) -> Result<bool, Error> {
        let result = sqlx::query(
            r#"
            UPDATE webhooks
            SET disabled = TRUE,
                disabled_reason = $3
            WHERE project_id = $1
              AND name = $2
              AND disabled = FALSE
            "#,
        )
        .bind(project_id)
        .bind(name)
        .bind(reason)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Re-enable a previously-disabled webhook (architecture §5.4:
    /// `ogeo webhook reenable <name>` is the only path back to active).
    /// Idempotent: returns `Ok(false)` when no disabled row existed.
    pub async fn reenable(
        &self,
        project_id: ProjectId,
        name: &str,
    ) -> Result<bool, Error> {
        let result = sqlx::query(
            r#"
            UPDATE webhooks
            SET disabled = FALSE,
                disabled_reason = NULL
            WHERE project_id = $1
              AND name = $2
              AND disabled = TRUE
            "#,
        )
        .bind(project_id)
        .bind(name)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Replace the encrypted secret material. Used by
    /// `ogeo webhook rotate-secret <name>` after the CLI generates a
    /// fresh per-webhook secret and reencrypts.
    pub async fn rotate_secret(
        &self,
        project_id: ProjectId,
        name: &str,
        new_secret_ciphertext: &str,
    ) -> Result<bool, Error> {
        let result = sqlx::query(
            r#"
            UPDATE webhooks
            SET secret_ciphertext = $3
            WHERE project_id = $1
              AND name = $2
            "#,
        )
        .bind(project_id)
        .bind(name)
        .bind(new_secret_ciphertext)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_webhook(row: sqlx::postgres::PgRow) -> Result<WebhookRow, Error> {
    Ok(WebhookRow {
        id: row.try_get("id")?,
        project_id: row.try_get("project_id")?,
        name: row.try_get("name")?,
        target_url: row.try_get("target_url")?,
        secret_ciphertext: row.try_get("secret_ciphertext")?,
        event_kinds: row.try_get("event_kinds")?,
        disabled: row.try_get("disabled")?,
        disabled_reason: row.try_get("disabled_reason")?,
        created_at: row.try_get("created_at")?,
    })
}
