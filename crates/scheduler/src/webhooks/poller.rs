//! Polling driver for the webhook dispatcher (Story 12.4).
//!
//! [`poll_once`] does one sweep:
//!
//! 1. List due deliveries via `WebhookDeliveryRepo::list_due`.
//! 2. For each row, fetch the owning webhook (target URL + active flag +
//!    secret ciphertext).
//! 3. Skip rows whose webhook has been disabled since the row was queued
//!    (manual disable or a previous auto-disable). Their delivery row
//!    stays `pending` until an operator reenables and a future poll
//!    picks them up, or until the queue trimming policy purges them
//!    (out of scope here).
//! 4. Spawn one `process_one_due` task per (row, webhook) tuple. Tasks
//!    are independent — a failing target does not block sibling
//!    deliveries (architecture §5.4 failure isolation NFR).
//!
//! Designed to be called by `apps/worker/src/main` on a tick interval.
//! Production wiring lives there; this module is the composable unit.

use anseo_storage::repositories::webhook_deliveries::PendingDelivery;
use anseo_storage::repositories::webhooks::WebhookRow;
use anseo_storage::Storage;
use reqwest::Client;
use std::time::Duration;
use tokio::task::JoinSet;

use crate::webhooks::tick::{process_one_due, DispatchResult, DispatcherError};

/// Default poll batch — bounds how many rows we fan-out per tick so a
/// deep backlog doesn't spike memory or peer the upstream HTTP layer.
pub const DEFAULT_BATCH_LIMIT: i64 = 64;

/// Default per-delivery HTTP timeout. The retry ladder's first step is
/// 1s, so a sustained backlog of slow consumers tops at one poll per
/// consumer-timeout.
pub const DEFAULT_DELIVERY_TIMEOUT: Duration = Duration::from_secs(30);

/// One polling sweep. Returns the per-delivery results so the worker's
/// tracing layer can emit structured logs without re-querying the DB.
pub async fn poll_once(
    storage: &Storage,
    http_client: &Client,
    batch_limit: i64,
    delivery_timeout: Duration,
) -> Result<Vec<DispatchResult>, DispatcherError> {
    let due = storage
        .webhook_deliveries()
        .list_due(chrono::Utc::now(), batch_limit)
        .await?;

    if due.is_empty() {
        return Ok(Vec::new());
    }

    // Pre-fetch the webhook rows the due deliveries reference. One query
    // per delivery is acceptable at batch_limit=64; a future optimization
    // could SELECT … WHERE id = ANY($1) for a single round trip.
    let mut tasks = JoinSet::new();
    for delivery in due {
        let webhook = match storage.webhooks().get_by_id(delivery.webhook_id).await? {
            Some(w) => w,
            None => {
                // Webhook row vanished — shouldn't happen because FK is
                // RESTRICT, but skip cleanly if it does.
                tracing::warn!(
                    event = "webhook.delivery_orphaned",
                    delivery_id = %delivery.id,
                    webhook_id = %delivery.webhook_id,
                    "delivery references missing webhook row; skipping"
                );
                continue;
            }
        };
        if webhook.disabled {
            tracing::debug!(
                event = "webhook.delivery_skipped_disabled",
                delivery_id = %delivery.id,
                webhook_id = %delivery.webhook_id,
                "delivery target is disabled; skipping until reenabled"
            );
            continue;
        }

        let storage_clone = storage.pool().clone();
        let http_clone = http_client.clone();
        tasks.spawn(spawn_one(
            storage_clone,
            http_clone,
            delivery,
            webhook,
            delivery_timeout,
        ));
    }

    let mut results = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(err)) => tracing::warn!(
                event = "webhook.dispatch_failed",
                error = %err,
                "process_one_due returned a transport error"
            ),
            Err(join_err) => tracing::error!(
                event = "webhook.task_panicked",
                error = %join_err,
                "dispatch task panicked"
            ),
        }
    }
    Ok(results)
}

async fn spawn_one(
    pool: sqlx::PgPool,
    http_client: Client,
    delivery: PendingDelivery,
    webhook: WebhookRow,
    delivery_timeout: Duration,
) -> Result<DispatchResult, DispatcherError> {
    let body = serde_json::to_vec(&delivery.payload_jsonb).map_err(|e| {
        DispatcherError::Storage(anseo_storage::Error::Sqlx(sqlx::Error::Decode(Box::new(e))))
    })?;
    let secret = decode_secret(&webhook.secret_ciphertext);
    let webhook_id_for_disable = webhook.id;
    process_one_due(
        &pool,
        &http_client,
        delivery,
        &webhook.target_url,
        &secret,
        &body,
        delivery_timeout,
        webhook_id_for_disable,
    )
    .await
}

/// Decode the stored secret_ciphertext back to bytes. Phase 2 single-host
/// uses RFC 4648 base64 (no at-rest encryption); a future story can
/// layer envelope encryption here without changing the call sites.
fn decode_secret(ciphertext: &str) -> Vec<u8> {
    // Minimal base64 decode — inverse of the CLI's encoder. Padding
    // tolerant. If decoding fails (operator manually inserted garbage),
    // return an empty secret so the consumer-side verify fails cleanly.
    base64_decode(ciphertext).unwrap_or_default()
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const REVERSE: [i16; 128] = build_reverse();
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut buf = [0i16; 4];
        let mut padding = 0;
        for slot in &mut buf {
            if i >= bytes.len() {
                return None;
            }
            let b = bytes[i];
            i += 1;
            if b == b'=' {
                padding += 1;
                *slot = 0;
                continue;
            }
            if b >= 128 {
                return None;
            }
            let v = REVERSE[b as usize];
            if v < 0 {
                return None;
            }
            *slot = v;
        }
        let n0 = ((buf[0] as u32) << 2) | ((buf[1] as u32) >> 4);
        let n1 = (((buf[1] as u32) & 0b1111) << 4) | ((buf[2] as u32) >> 2);
        let n2 = (((buf[2] as u32) & 0b11) << 6) | (buf[3] as u32);
        out.push(n0 as u8);
        if padding < 2 {
            out.push(n1 as u8);
        }
        if padding < 1 {
            out.push(n2 as u8);
        }
    }
    Some(out)
}

const fn build_reverse() -> [i16; 128] {
    let mut table = [-1i16; 128];
    let alpha = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut i = 0;
    while i < alpha.len() {
        table[alpha[i] as usize] = i as i16;
        i += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decode_round_trip_rfc_4648_vectors() {
        // Inverse of the CLI's encode tests. Pinned across alphabet
        // changes so any tweak surfaces on both sides.
        let cases = [
            ("", &b""[..]),
            ("Zg==", &b"f"[..]),
            ("Zm8=", &b"fo"[..]),
            ("Zm9v", &b"foo"[..]),
            ("Zm9vYg==", &b"foob"[..]),
            ("Zm9vYmE=", &b"fooba"[..]),
            ("Zm9vYmFy", &b"foobar"[..]),
        ];
        for (encoded, expected) in cases {
            let decoded = base64_decode(encoded).unwrap();
            assert_eq!(decoded.as_slice(), expected, "decode of {encoded}");
        }
    }

    #[test]
    fn base64_decode_rejects_garbage() {
        assert!(base64_decode("not valid!").is_none());
        assert!(base64_decode("ZZZ").is_none()); // not multiple of 4
    }

    #[test]
    fn decode_secret_returns_empty_on_garbage() {
        // Defensive: an operator who edits the DB directly with garbage
        // gets a verify-fail on the consumer rather than a panic in the
        // dispatcher.
        assert_eq!(decode_secret("definitely not base64"), Vec::<u8>::new());
    }

    #[test]
    fn defaults_match_arch_5_intent() {
        // batch limit bounded so a deep backlog doesn't peer upstream;
        // 30s per-call timeout matches the 1s first retry step.
        assert_eq!(DEFAULT_BATCH_LIMIT, 64);
        assert_eq!(DEFAULT_DELIVERY_TIMEOUT, Duration::from_secs(30));
    }
}
