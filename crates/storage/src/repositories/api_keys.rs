//! Repository for the Phase 2 Story 12.1 `api_keys` table.
//!
//! Uses the runtime `sqlx::query` form (not the compile-time-validated
//! `query!` macros) because Story 12.1 lands without a `.sqlx/` offline
//! cache entry for the new migration. A follow-up pass with a live
//! DATABASE_URL can regenerate the cache and convert to `query!` if the
//! project decides to enforce compile-time SQL across Phase 2 storage.

use chrono::{DateTime, Utc};
use opengeo_core::ids::ProjectId;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

/// Operator-visible row for the CLI's `ogeo api key list` and any future
/// dashboard surface. The `sha256_hash` column exists in the table but is
/// intentionally absent from this struct — the hash is the DB lookup key,
/// so exposing it in logs would let an attacker with read access to the
/// data confirm candidate plaintexts offline. Internal callers that need
/// the hash should use the dedicated `lookup_active_project` / `revoke`
/// methods instead of `list_for_project`.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub project_id: ProjectId,
    pub name: String,
    pub prefix: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct ApiKeyRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ApiKeyRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Insert a freshly-generated key. The plaintext is the caller's
    /// responsibility to show ONCE to the user and discard; this repo only
    /// sees the sha256 hash + display prefix.
    ///
    /// Duplicate `(project_id, name)` surfaces as a `sqlx::Error::Database`
    /// (UNIQUE violation) — we deliberately do not pre-check; one round
    /// trip is cheaper than two and the DB is authoritative regardless.
    pub async fn insert(
        &self,
        project_id: ProjectId,
        name: &str,
        sha256_hash: &str,
        prefix: &str,
    ) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"
            INSERT INTO api_keys (id, project_id, name, sha256_hash, prefix)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(name)
        .bind(sha256_hash)
        .bind(prefix)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Look up the project_id for a sha256 hash, only if the key is active
    /// (revoked_at IS NULL). The hot path the API auth middleware calls on
    /// every request. Index `idx_api_keys_sha256_active` covers the
    /// `WHERE revoked_at IS NULL` predicate so this is O(1) regardless of
    /// the revoked-row count.
    pub async fn lookup_active_project(
        &self,
        sha256_hash: &str,
    ) -> Result<Option<ProjectId>, Error> {
        let row = sqlx::query(
            r#"
            SELECT project_id
            FROM api_keys
            WHERE sha256_hash = $1
              AND revoked_at IS NULL
            LIMIT 1
            "#,
        )
        .bind(sha256_hash)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| r.try_get("project_id")).transpose()?)
    }

    /// Record successful authentication. Fire-and-forget from the
    /// middleware's perspective — we update `last_used_at` after the auth
    /// decision has already been returned to the handler.
    pub async fn touch_last_used(&self, sha256_hash: &str) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE api_keys
            SET last_used_at = now()
            WHERE sha256_hash = $1
              AND revoked_at IS NULL
            "#,
        )
        .bind(sha256_hash)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// List rows visible to operators. Includes revoked rows for audit;
    /// callers can filter. Ordered by creation desc so the most recent key
    /// is first.
    pub async fn list_for_project(&self, project_id: ProjectId) -> Result<Vec<ApiKeyRow>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, project_id, name, prefix, last_used_at,
                   revoked_at, revoked_reason, created_at
            FROM api_keys
            WHERE project_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(project_id)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                Ok(ApiKeyRow {
                    id: r.try_get("id")?,
                    project_id: r.try_get("project_id")?,
                    name: r.try_get("name")?,
                    prefix: r.try_get("prefix")?,
                    last_used_at: r.try_get("last_used_at")?,
                    revoked_at: r.try_get("revoked_at")?,
                    revoked_reason: r.try_get("revoked_reason")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    /// Count active (non-revoked) keys for one project. Used by
    /// `apps/api/src/main.rs` to gate non-loopback binds for THIS project —
    /// a key for an unrelated project must not unlock a public bind for a
    /// different project's API.
    pub async fn count_active_for_project(&self, project_id: ProjectId) -> Result<i64, Error> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM api_keys
            WHERE project_id = $1
              AND revoked_at IS NULL
            "#,
        )
        .bind(project_id)
        .fetch_one(self.pool)
        .await?;
        Ok(row.try_get("count")?)
    }

    /// Soft-revoke by name. Returns Ok(true) if a row was updated, Ok(false)
    /// if no active key with that name existed (idempotent revoke).
    pub async fn revoke(
        &self,
        project_id: ProjectId,
        name: &str,
        reason: Option<&str>,
    ) -> Result<bool, Error> {
        let rows = sqlx::query(
            r#"
            UPDATE api_keys
            SET revoked_at = now(),
                revoked_reason = $3
            WHERE project_id = $1
              AND name = $2
              AND revoked_at IS NULL
            "#,
        )
        .bind(project_id)
        .bind(name)
        .bind(reason)
        .execute(self.pool)
        .await?;
        Ok(rows.rows_affected() > 0)
    }
}
