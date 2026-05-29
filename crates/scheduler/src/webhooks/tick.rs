//! Orchestration layer for the Phase 2 webhook dispatcher (Story 12.4).
//!
//! [`process_one_due`] is the composable unit: hand it the storage pool,
//! a reqwest client, the pending delivery row, the already-decrypted
//! per-webhook secret, the target URL, and the event payload bytes; it
//! signs, fires, updates the row to its terminal state, and bubbles up
//! the outcome. The caller is responsible for:
//!
//! - Polling `WebhookDeliveryRepo::list_due` for due rows.
//! - Looking up the corresponding `webhooks` row to extract `target_url`
//!   and `secret_ciphertext`, then decrypting the secret (the in-tree
//!   `opengeo_core::secret_store` keychain backend is the Phase 2 default).
//! - Reconstructing the event payload bytes for `event_id` + `event_kind`
//!   (currently a caller concern; a future additive migration can add a
//!   payload column to `webhook_deliveries` to lock immutability across
//!   retries).
//! - Spawning per-(event, webhook) tokio tasks for fan-out — the
//!   per-target task cardinality is what realizes the architecture
//!   §5.4 failure-isolation guarantee.
//!
//! Auto-disable: when a delivery transitions to `dropped`, this function
//! checks `count_consecutive_dropped` and disables the webhook when the
//! threshold trips. The CLI's `ogeo webhook reenable` is the only path
//! back to active per architecture §5.4.

use chrono::{Duration as ChronoDuration, Utc};
use opengeo_storage::repositories::webhook_deliveries::PendingDelivery;
use reqwest::Client;
use sqlx::PgPool;
use std::time::Duration;

use crate::webhooks::dispatcher::{deliver_one, DeliveryOutcome};
use crate::webhooks::retry::{next_delay, should_auto_disable};

#[derive(Debug, thiserror::Error)]
pub enum DispatcherError {
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("storage error")]
    Storage(#[from] opengeo_storage::Error),
}

/// Result returned to the caller of [`process_one_due`]. Mirrors the
/// repo's row transitions, with auto-disable surfaced when it tripped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchResult {
    Delivered,
    Retrying,
    DroppedPermanent,
    DroppedAndWebhookAutoDisabled,
}

/// Fire one due delivery end-to-end: sign, POST, classify, transition
/// the row. Returns a structured outcome for the caller (typically a
/// fanned-out tokio task) so it can log, surface to ops dashboards, or
/// trigger downstream effects without re-querying the row state.
#[allow(clippy::too_many_arguments)]
pub async fn process_one_due(
    pool: &PgPool,
    http_client: &Client,
    delivery: PendingDelivery,
    target_url: &str,
    secret: &[u8],
    body: &[u8],
    timeout: Duration,
    webhook_id_for_disable_check: uuid::Uuid,
) -> Result<DispatchResult, DispatcherError> {
    let storage = opengeo_storage::Storage::from_pool(pool.clone());
    let now = Utc::now();
    let timestamp_unix = now.timestamp();

    let (outcome, snippet) = deliver_one(
        http_client,
        target_url,
        secret,
        body,
        timestamp_unix,
        timeout,
    )
    .await;

    match outcome {
        DeliveryOutcome::Delivered { status } => {
            storage
                .webhook_deliveries()
                .mark_delivered(delivery.id, status as i32, Some(&snippet))
                .await?;
            Ok(DispatchResult::Delivered)
        }
        DeliveryOutcome::RetryableFailure { status, .. } => {
            let attempts_so_far = delivery.attempt.max(0) as u32;
            match next_delay(attempts_so_far) {
                Some(wait) => {
                    let next_at = now
                        + ChronoDuration::from_std(wait).unwrap_or(ChronoDuration::seconds(60));
                    storage
                        .webhook_deliveries()
                        .mark_failed_retryable(
                            delivery.id,
                            status.map(|s| s as i32),
                            Some(&snippet),
                            next_at,
                        )
                        .await?;
                    Ok(DispatchResult::Retrying)
                }
                None => {
                    // Ladder exhausted — drop and re-check auto-disable.
                    drop_and_maybe_disable(
                        &storage,
                        delivery.id,
                        status.map(|s| s as i32),
                        &snippet,
                        webhook_id_for_disable_check,
                    )
                    .await
                }
            }
        }
        DeliveryOutcome::PermanentFailure { status, .. } => {
            // Permanent 4xx skips the ladder entirely.
            drop_and_maybe_disable(
                &storage,
                delivery.id,
                Some(status as i32),
                &snippet,
                webhook_id_for_disable_check,
            )
            .await
        }
    }
}

async fn drop_and_maybe_disable(
    storage: &opengeo_storage::Storage,
    delivery_id: uuid::Uuid,
    response_status: Option<i32>,
    snippet: &str,
    webhook_id: uuid::Uuid,
) -> Result<DispatchResult, DispatcherError> {
    storage
        .webhook_deliveries()
        .mark_dropped(delivery_id, response_status, Some(snippet))
        .await?;

    let consecutive = storage
        .webhook_deliveries()
        .count_consecutive_dropped(webhook_id)
        .await?;

    if should_auto_disable(consecutive.max(0) as u32) {
        // The auto-disable is fire-and-best-effort: a race with a
        // concurrent reenable would simply re-disable, which is
        // acceptable. Failure to write the disable does NOT change the
        // delivery's terminal state.
        let _ = sqlx::query(
            r#"
            UPDATE webhooks
            SET disabled = TRUE,
                disabled_reason = $2
            WHERE id = $1
              AND disabled = FALSE
            "#,
        )
        .bind(webhook_id)
        .bind(format!(
            "auto-disabled after {consecutive} consecutive permanent-failed deliveries"
        ))
        .execute(storage.pool())
        .await;
        return Ok(DispatchResult::DroppedAndWebhookAutoDisabled);
    }
    Ok(DispatchResult::DroppedPermanent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webhooks::retry::AUTO_DISABLE_THRESHOLD;

    // No live-DB tests here — the IO surface (deliver_one, sqlx repo
    // calls) is exercised in the dedicated dispatcher.rs unit tests
    // and the to-be-written wiremock integration suite. These pin the
    // dispatch-result enum semantics so they don't drift.

    #[test]
    fn dispatch_result_variants_distinct() {
        let variants = [
            DispatchResult::Delivered,
            DispatchResult::Retrying,
            DispatchResult::DroppedPermanent,
            DispatchResult::DroppedAndWebhookAutoDisabled,
        ];
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn auto_disable_threshold_is_5_per_arch_5_4() {
        // Pin the cross-module invariant: the retry module's threshold
        // must match the tick module's expectation. Drift between them
        // would silently change operator-visible behavior.
        assert_eq!(AUTO_DISABLE_THRESHOLD, 5);
    }
}
