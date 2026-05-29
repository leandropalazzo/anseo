//! Repository for the Phase 2 `webhook_deliveries` table (Story 12.4).
//!
//! State machine per row:
//!
//! ```text
//! pending ──> delivered            (HTTP 2xx)
//!       └──> failed                (HTTP 4xx-non-perm or 5xx; will retry)
//!       └──> dropped               (5 attempts exhausted; failed_permanent
//!                                   semantics)
//! ```
//!
//! The dispatcher reads `pending` rows whose `next_attempt_at` has passed
//! and a single `(webhook_id, event_id, attempt)` claim wins the race via
//! the storage row's update predicates — we deliberately keep the claim
//! protocol simple here and rely on the dispatcher's per-target task
//! cardinality to bound contention.
//!
//! Like the `api_keys` repo, this uses the runtime `sqlx::query` form
//! since the migration shipped without a `.sqlx/` offline cache entry.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

/// Wire-stable status strings; mirror the migration's CHECK constraint.
pub mod status {
    pub const PENDING: &str = "pending";
    pub const DELIVERED: &str = "delivered";
    pub const FAILED: &str = "failed";
    pub const DROPPED: &str = "dropped";
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event_id: Uuid,
    pub event_kind: String,
    pub attempt: i32,
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// Frozen at insert time so retries send byte-identical payloads.
    pub payload_jsonb: serde_json::Value,
}

pub struct WebhookDeliveryRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> WebhookDeliveryRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Record a new delivery row in `pending`. The caller (dispatcher)
    /// generates `event_id` once per logical event so retried rows share
    /// it — that's the idempotency key downstream consumers see.
    pub async fn insert_pending(
        &self,
        webhook_id: Uuid,
        event_id: Uuid,
        event_kind: &str,
        attempt: i32,
        next_attempt_at: Option<DateTime<Utc>>,
        payload_jsonb: &serde_json::Value,
    ) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"
            INSERT INTO webhook_deliveries
                (id, webhook_id, event_id, event_kind, attempt, status,
                 next_attempt_at, payload_jsonb)
            VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7)
            "#,
        )
        .bind(id)
        .bind(webhook_id)
        .bind(event_id)
        .bind(event_kind)
        .bind(attempt)
        .bind(next_attempt_at)
        .bind(payload_jsonb)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Mark a delivery row `delivered`. Idempotent against already-terminal
    /// rows (the UPDATE is a no-op if the row already left `pending`).
    pub async fn mark_delivered(
        &self,
        id: Uuid,
        response_status: i32,
        response_body_snippet: Option<&str>,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET status = 'delivered',
                response_status = $2,
                response_body_snippet = $3,
                next_attempt_at = NULL
            WHERE id = $1
              AND status = 'pending'
            "#,
        )
        .bind(id)
        .bind(response_status)
        .bind(response_body_snippet)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Mark a delivery row `failed` (retryable). Records the next attempt
    /// time so the dispatcher can pick it up at the right moment.
    pub async fn mark_failed_retryable(
        &self,
        id: Uuid,
        response_status: Option<i32>,
        response_body_snippet: Option<&str>,
        next_attempt_at: DateTime<Utc>,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET status = 'failed',
                response_status = $2,
                response_body_snippet = $3,
                next_attempt_at = $4
            WHERE id = $1
              AND status = 'pending'
            "#,
        )
        .bind(id)
        .bind(response_status)
        .bind(response_body_snippet)
        .bind(next_attempt_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Mark a delivery row `dropped` (permanent — retry ladder exhausted).
    /// The webhook's auto-disable threshold is checked against the
    /// resulting count via `count_consecutive_dropped` by the caller.
    pub async fn mark_dropped(
        &self,
        id: Uuid,
        response_status: Option<i32>,
        response_body_snippet: Option<&str>,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET status = 'dropped',
                response_status = $2,
                response_body_snippet = $3,
                next_attempt_at = NULL
            WHERE id = $1
              AND status = 'pending'
            "#,
        )
        .bind(id)
        .bind(response_status)
        .bind(response_body_snippet)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// List pending rows whose `next_attempt_at` has arrived. Bounded by
    /// `limit` so the dispatcher polls bounded batches under a deep
    /// backlog. Index `idx_webhook_deliveries_pending` covers the
    /// `WHERE status = 'pending'` filter so this is O(log n).
    pub async fn list_due(
        &self,
        now: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<PendingDelivery>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, webhook_id, event_id, event_kind, attempt,
                   next_attempt_at, payload_jsonb
            FROM webhook_deliveries
            WHERE status = 'pending'
              AND (next_attempt_at IS NULL OR next_attempt_at <= $1)
            ORDER BY next_attempt_at NULLS FIRST, created_at
            LIMIT $2
            "#,
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                Ok(PendingDelivery {
                    id: r.try_get("id")?,
                    webhook_id: r.try_get("webhook_id")?,
                    event_id: r.try_get("event_id")?,
                    event_kind: r.try_get("event_kind")?,
                    attempt: r.try_get("attempt")?,
                    next_attempt_at: r.try_get("next_attempt_at")?,
                    payload_jsonb: r.try_get("payload_jsonb")?,
                })
            })
            .collect()
    }

    /// Count consecutive permanently-dropped deliveries on this webhook
    /// from the most recent backwards — stops counting at the first
    /// non-dropped row. Used by the dispatcher to decide whether to
    /// auto-disable the webhook (architecture §5.4: 5 consecutive
    /// permanent failures → disabled).
    pub async fn count_consecutive_dropped(
        &self,
        webhook_id: Uuid,
    ) -> Result<i64, Error> {
        // Window over (webhook_id, created_at desc). The first non-
        // 'dropped' row terminates the count. Doable in pure SQL via a
        // CASE-in-CTE but the most-recent-N pattern is simpler.
        let row = sqlx::query(
            r#"
            WITH recent AS (
                SELECT status,
                       ROW_NUMBER() OVER (ORDER BY created_at DESC) AS rn
                FROM webhook_deliveries
                WHERE webhook_id = $1
                ORDER BY created_at DESC
                LIMIT 50
            ),
            terminated AS (
                SELECT rn
                FROM recent
                WHERE status <> 'dropped'
                ORDER BY rn ASC
                LIMIT 1
            )
            SELECT COALESCE((SELECT rn - 1 FROM terminated),
                            (SELECT COUNT(*) FROM recent)) AS count
            "#,
        )
        .bind(webhook_id)
        .fetch_one(self.pool)
        .await?;
        Ok(row.try_get("count")?)
    }
}
