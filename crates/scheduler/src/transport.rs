//! Cross-process event transport via Postgres LISTEN/NOTIFY.
//!
//! The Phase 2 lifecycle event flow has two endpoints that live in different
//! processes:
//!
//! - The worker (`apps/worker`) emits events when it claims, completes,
//!   fails, caps, rolls forward, misses, or debounces a tick.
//! - The API (`apps/api`) hosts the SSE endpoint subscribers connect to, and
//!   (later) the webhook dispatcher and notification channels.
//!
//! A `tokio::sync::broadcast` channel cannot span processes, so the worker
//! publishes via Postgres `pg_notify('opengeo_events', <json>)` and the API
//! runs a long-lived `PgListener` task that forwards each NOTIFY into the
//! API-process broadcast channel. SSE / webhook / notification subscribers
//! then read from that broadcast as before.
//!
//! The architectural contract:
//! - Channel name `opengeo_events` is stable for the lifetime of Phase 2.
//! - Payload is `LifecycleEvent` serialized as JSON (the same wire shape
//!   that SSE consumers see).
//! - Postgres caps the NOTIFY payload at 8000 bytes; Phase 2 events fit
//!   well under that. Larger Phase 3 events (LLM-aided anomaly with
//!   evidence blobs) will need a side-channel table — out of scope here.

use crate::events::LifecycleEvent;
use sqlx::postgres::{PgListener, PgPool};
use tokio::sync::broadcast;

/// Postgres NOTIFY channel name carrying ARCH-17 lifecycle events.
pub const EVENTS_CHANNEL: &str = "opengeo_events";

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("serialization error")]
    Serialize(#[from] serde_json::Error),
}

/// Publish one lifecycle event to all listening processes. Fire-and-forget at
/// the broadcast layer (any subscriber that's behind drops oldest events),
/// but the NOTIFY itself is durable for the transaction it lands in: callers
/// MUST call `publish` from within (or immediately after committing) the
/// transaction that records the underlying state change, so a worker crash
/// after state-change-commit but before NOTIFY does not silently lose events.
///
/// In practice Story 10.2's worker emits NOTIFY in the same connection as the
/// `claim_tick` / state-update queries, which is sufficient.
pub async fn publish(pool: &PgPool, event: &LifecycleEvent) -> Result<(), TransportError> {
    let payload = serde_json::to_string(event)?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(EVENTS_CHANNEL)
        .bind(payload)
        .execute(pool)
        .await?;
    Ok(())
}

/// Long-lived listener that forwards every NOTIFY on `EVENTS_CHANNEL` into
/// `sender`. Returns only on fatal listener error; callers should run it as
/// a `tokio::spawn`-ed task and restart on error.
///
/// Malformed payloads are logged and skipped — a single bad payload must not
/// stop the listener (forward compatibility: a Phase 3 emitter may send an
/// event shape the Phase 2 listener cannot parse).
pub async fn listen(
    database_url: &str,
    sender: broadcast::Sender<LifecycleEvent>,
) -> Result<(), TransportError> {
    let mut listener = PgListener::connect(database_url).await?;
    listener.listen(EVENTS_CHANNEL).await?;
    tracing::info!(
        event = "transport.listener_ready",
        channel = EVENTS_CHANNEL,
        "lifecycle event listener attached"
    );
    loop {
        let notification = listener.recv().await?;
        match serde_json::from_str::<LifecycleEvent>(notification.payload()) {
            Ok(evt) => {
                let _ = sender.send(evt);
            }
            Err(err) => tracing::warn!(
                event = "transport.parse_error",
                error = %err,
                payload = notification.payload(),
                "ignoring malformed lifecycle event"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{LifecycleEvent, SchedulePayload};
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_event() -> LifecycleEvent {
        LifecycleEvent::TickPlanned(SchedulePayload {
            event_id: Uuid::nil(),
            project_id: Uuid::nil(),
            schedule_id: Uuid::nil(),
            schedule_name: "daily-check".into(),
            tick_id: Uuid::nil(),
            tick_ts: Utc::now(),
            emitted_at: Utc::now(),
        })
    }

    #[test]
    fn channel_name_is_stable_wire_constant() {
        // Story 10.2 architectural contract: changing this string would
        // de-sync deployed worker + api binaries at runtime.
        assert_eq!(EVENTS_CHANNEL, "opengeo_events");
    }

    #[test]
    fn publish_payload_round_trips_through_serde() {
        // Exercises the encode side of publish without needing a live pool.
        // The decode side runs inside the listener loop and is the same
        // serde_json call shape; covered by the events.rs round-trip tests.
        let evt = sample_event();
        let payload = serde_json::to_string(&evt).unwrap();
        let back: LifecycleEvent = serde_json::from_str(&payload).unwrap();
        assert_eq!(back, evt);
    }

    #[test]
    fn payload_fits_postgres_notify_8k_limit() {
        // Postgres caps NOTIFY payload at ~8000 bytes. A SchedulePayload
        // with realistic field lengths is well under; verify here so a
        // future field addition doesn't silently overflow.
        let evt = LifecycleEvent::TickPlanned(SchedulePayload {
            event_id: Uuid::nil(),
            project_id: Uuid::nil(),
            schedule_id: Uuid::nil(),
            schedule_name: "a-fairly-long-schedule-name-that-someone-might-actually-use".into(),
            tick_id: Uuid::nil(),
            tick_ts: Utc::now(),
            emitted_at: Utc::now(),
        });
        let payload = serde_json::to_string(&evt).unwrap();
        assert!(
            payload.len() < 6000,
            "lifecycle event payload {} bytes — getting close to NOTIFY 8000-byte cap; \
             consider a side-channel table for larger Phase 3 events",
            payload.len()
        );
    }
}
