//! ARCH-16 — SSE event stream for live worker / webhook / anomaly events.
//!
//! Route: `GET /v1/projects/:project_id/events`.
//!
//! Story 10.2 ships the stream itself. The cross-process transport
//! (`opengeo_scheduler::transport`) bridges worker NOTIFY → API broadcast
//! channel → SSE subscribers. Auth defers to Story 12.1; until then the
//! API binary should be bound to localhost (the Compose default).
//!
//! Wire shape: each ARCH-17 event becomes one `event: <kind>` /
//! `data: <json>` SSE message. The `<kind>` matches
//! `LifecycleEvent::kind()`. The route filters the broadcast by
//! `LifecycleEvent::project_id() == path project_id` so a Phase 4
//! multi-project deployment does not leak events across projects via the
//! shared broadcast channel.
//!
//! Lagged subscribers: `BroadcastStreamRecvError::Lagged(n)` surfaces as a
//! `lagged` SSE event so the connector (UX-DR77) can choose to reconnect
//! and replay rather than silently miss messages.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Extension;
use axum::Router;
use futures::stream::StreamExt;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

/// Mount the events route at its absolute path. Used when the route is
/// merged directly into the root router.
pub fn router() -> Router<AppState> {
    Router::new().route("/v1/projects/:project_id/events", get(events_stream))
}

/// Mount the events route relative to `/v1` so it can be `Router::nest`ed
/// alongside the rest of the Phase 2 `/v1/*` surface. The two helpers are
/// mutually exclusive — `apps/api/src/lib.rs` picks one.
pub fn router_under_v1_relative() -> Router<AppState> {
    Router::new().route("/projects/:project_id/events", get(events_stream))
}

async fn events_stream(
    Path(project_id): Path<Uuid>,
    Extension(AuthenticatedProject(authenticated_pid)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    // Cross-project guard: the path-bound project_id MUST equal the key's
    // authenticated scope. Mismatch → 403, not 404, so the caller learns
    // their key is valid but not for this project.
    let auth_uuid = Uuid::from_bytes(authenticated_pid.into_ulid().to_bytes());
    if auth_uuid != project_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let rx = state.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |item| async move {
        match item {
            Ok(event) => {
                if event.project_id() != project_id {
                    return None;
                }
                let payload = serde_json::to_string(&event).ok()?;
                Some(Ok::<_, Infallible>(
                    Event::default().event(event.kind()).data(payload),
                ))
            }
            Err(BroadcastStreamRecvError::Lagged(n)) => {
                // Subscriber fell behind the broadcast buffer. Surface this
                // as a `lagged` SSE event so the UX-DR77 connector can
                // reconnect/replay rather than miss state changes silently.
                Some(Ok(Event::default()
                    .event("lagged")
                    .data(format!("{{\"dropped\":{n}}}"))))
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use opengeo_scheduler::events::{LifecycleEvent, SchedulePayload};
    use opengeo_scheduler::worker::event_channel;
    use uuid::Uuid;

    fn payload_for(project_id: Uuid) -> SchedulePayload {
        SchedulePayload {
            event_id: Uuid::nil(),
            project_id,
            schedule_id: Uuid::nil(),
            schedule_name: "test".into(),
            tick_id: Uuid::nil(),
            tick_ts: Utc::now(),
            emitted_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn broadcast_stream_yields_serialized_event() {
        let (tx, rx) = event_channel();
        let evt = LifecycleEvent::TickPlanned(payload_for(Uuid::nil()));
        tx.send(evt.clone()).unwrap();
        let mut stream = BroadcastStream::new(rx);
        let received = stream.next().await.unwrap().unwrap();
        assert_eq!(received, evt);
    }

    #[tokio::test]
    async fn project_filter_drops_cross_project_events() {
        // Two events for two different projects on the same broadcast.
        // The subscriber's path-bound project_id must only see its own.
        let (tx, rx) = event_channel();
        let mine = Uuid::from_u128(1);
        let theirs = Uuid::from_u128(2);
        tx.send(LifecycleEvent::TickPlanned(payload_for(theirs)))
            .unwrap();
        tx.send(LifecycleEvent::TickPlanned(payload_for(mine)))
            .unwrap();
        // Manually exercise the same filter the route applies.
        let mut filtered: Vec<LifecycleEvent> = BroadcastStream::new(rx)
            .take(2)
            .filter_map(|item| async move { item.ok() })
            .filter(|evt| {
                let pid = evt.project_id();
                async move { pid == mine }
            })
            .collect()
            .await;
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered.pop().unwrap().project_id(), mine);
    }
}
