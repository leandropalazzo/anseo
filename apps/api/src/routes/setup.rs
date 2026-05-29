#![allow(clippy::doc_overindented_list_items)]
//! Story 15.1 — `/v1/setup/*` backend endpoints.
//!
//! Three endpoints (this story; the remaining 5 land in later 15.x):
//!
//! - `GET  /v1/setup/status` — synchronous probe of all sections
//!                             (Postgres, ClickHouse, worker, webhook
//!                             target, API keys, Docker). Always 200;
//!                             per-section `state: "unknown"` on failure.
//! - `POST /v1/setup/clickhouse/install` — kicks off a mock background
//!                             state machine and returns 202 + `install_id`.
//! - `GET  /v1/setup/clickhouse/install-stream?id=<ulid>` — SSE stream
//!                             of `{ step, progress, log_line }` events.
//!
//! **Mock state machine warning.** The install state machine in this
//! file is a **deterministic mock** — it sleeps between steps so the SSE
//! stream shows progression, but it does NOT shell out to Docker, does
//! NOT pull an image, and does NOT apply ClickHouse migrations. The
//! real Docker calls land in Story 15.3 (ClickHouse local install flow).
//! Everything in `run_mock_install` below is placeholder behaviour with
//! the wire shape locked so the frontend (Story 15.3) can be built
//! against a stable contract.
//!
//! Auth: routes mount inside the standard `/v1` auth gate (OQ-P3-24
//! default — same API key as the rest of `/v1`). The `X-OpenGEO-Project`
//! header is accepted-but-ignored (same as every other v1 route, per
//! decision L2 / Story 0.11).

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use futures::stream;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::setup_probe::{probe_all, SetupStatus};
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(get_setup_status))
        .route("/setup/clickhouse/install", post(post_clickhouse_install))
        .route(
            "/setup/clickhouse/install-stream",
            get(get_clickhouse_install_stream),
        )
}

// ---------------------------------------------------------------------
// GET /v1/setup/status
// ---------------------------------------------------------------------

async fn get_setup_status(State(state): State<AppState>) -> Json<SetupStatus> {
    Json(probe_all(&state).await)
}

// ---------------------------------------------------------------------
// POST /v1/setup/clickhouse/install
// ---------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct InstallAccepted {
    pub install_id: String,
    pub stream: String,
}

/// Steps the mock state machine walks through. Order is the canonical
/// install sequence ratified in architecture §4.5. Frontend uses these
/// strings as i18n keys; do not rename without coordinating with the
/// `/setup` UI (Story 15.3).
pub const INSTALL_STEPS: &[&str] = &[
    "docker_detected",
    "image_pulling",
    "container_starting",
    "provisioning_user",
    "applying_migrations",
    "running_parity_test",
    "complete",
];

/// In-memory progress record for a single install. Lives in `AppState`.
#[derive(Debug, Clone, Serialize)]
pub struct InstallState {
    pub install_id: String,
    pub started_at: DateTime<Utc>,
    pub events: Vec<InstallEvent>,
    /// Set to `"complete"` or `"failed"` when the state machine
    /// terminates. While running, the last entry of `events.step` is
    /// the live step.
    pub terminal: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallEvent {
    pub step: String,
    pub progress: f32,
    pub log_line: String,
    pub at: DateTime<Utc>,
}

async fn post_clickhouse_install(
    State(state): State<AppState>,
) -> (StatusCode, Json<InstallAccepted>) {
    let install_id = Ulid::new();
    let id_str = install_id.to_string();
    let now = Utc::now();
    {
        let mut guard = state.setup_install_state.write().await;
        guard.insert(
            install_id,
            InstallState {
                install_id: id_str.clone(),
                started_at: now,
                events: Vec::new(),
                terminal: None,
            },
        );
    }
    // ────────────────────────────────────────────────────────────────
    // MOCK STATE MACHINE — see module-level comment. This spawns a
    // background task that walks INSTALL_STEPS with deterministic
    // delays. Replaced in Story 15.3 with real Docker calls.
    // ────────────────────────────────────────────────────────────────
    let map = state.setup_install_state.clone();
    tokio::spawn(async move {
        run_mock_install(install_id, map).await;
    });

    let resp = InstallAccepted {
        install_id: id_str.clone(),
        stream: format!("/v1/setup/clickhouse/install-stream?id={id_str}"),
    };
    (StatusCode::ACCEPTED, Json(resp))
}

/// **MOCK** — deterministic delays per step so the SSE stream is
/// observably progressing in tests + dev. Real implementation in 15.3.
async fn run_mock_install(
    install_id: Ulid,
    map: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<Ulid, InstallState>>>,
) {
    // Keep the mock fast enough for tests but slow enough that an SSE
    // client sees individual events rather than a single batched burst.
    // Tests assert ≥ 3 events; total wall-clock is ~210ms across 7
    // steps, well within the test harness's default budget.
    let step_delay = Duration::from_millis(30);

    for (idx, step) in INSTALL_STEPS.iter().enumerate() {
        tokio::time::sleep(step_delay).await;
        let progress = (idx as f32 + 1.0) / INSTALL_STEPS.len() as f32;
        let event = InstallEvent {
            step: (*step).to_string(),
            progress,
            log_line: format!("[mock] {step} — Story 15.1 placeholder; real impl lands in 15.3"),
            at: Utc::now(),
        };
        let mut guard = map.write().await;
        if let Some(s) = guard.get_mut(&install_id) {
            s.events.push(event);
            if *step == "complete" {
                s.terminal = Some("complete".to_string());
            }
        } else {
            // Install record was dropped (probably a test reset);
            // stop walking.
            return;
        }
    }
}

// ---------------------------------------------------------------------
// GET /v1/setup/clickhouse/install-stream?id=<ulid>
// ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InstallStreamQuery {
    pub id: String,
}

async fn get_clickhouse_install_stream(
    State(state): State<AppState>,
    Query(q): Query<InstallStreamQuery>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<serde_json::Value>),
> {
    let id = Ulid::from_string(&q.id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_install_id",
                "message": "`id` must be a valid ULID"
            })),
        )
    })?;
    // 404 if no install record — this catches both unknown ids and
    // ids whose state was evicted (we do not evict today, but a future
    // janitor might).
    {
        let guard = state.setup_install_state.read().await;
        if !guard.contains_key(&id) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "install_not_found",
                    "message": "no install with that id; POST /v1/setup/clickhouse/install first"
                })),
            ));
        }
    }

    // Polling stream: every 25ms we read the install record, emit any
    // new events since `cursor`, and stop when `terminal` is set AND
    // we've emitted everything. This is simpler than wiring per-install
    // broadcast channels for the mock; Story 15.3 may upgrade to a
    // proper channel if the real install needs lower latency.
    let map = state.setup_install_state.clone();
    let stream = stream::unfold(
        (map, id, 0usize, false),
        |(map, id, cursor, finished)| async move {
            if finished {
                return None;
            }
            loop {
                let (next_cursor, batch, terminal) = {
                    let guard = map.read().await;
                    #[allow(clippy::question_mark)] // closure must return Option, not propagate
                    let Some(s) = guard.get(&id) else {
                        return None;
                    };
                    let batch: Vec<InstallEvent> = s.events[cursor..].to_vec();
                    let next_cursor = s.events.len();
                    (next_cursor, batch, s.terminal.clone())
                };
                if !batch.is_empty() {
                    // Emit the first event in this batch; remaining
                    // batched events become subsequent unfold ticks.
                    let mut events_iter = batch.into_iter();
                    let first = events_iter.next().unwrap();
                    let payload = serde_json::to_string(&first)
                        .unwrap_or_else(|_| "{}".to_string());
                    let evt = Event::default().event("install").data(payload);
                    // If this was the last event AND the machine is
                    // terminal, signal finished so the next unfold tick
                    // returns None and closes the stream.
                    let is_last = cursor + 1 == next_cursor && terminal.is_some();
                    return Some((Ok(evt), (map, id, cursor + 1, is_last)));
                }
                if terminal.is_some() {
                    // No new events and terminal — close.
                    return None;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        },
    );
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_steps_terminate_with_complete() {
        // Wire contract: `complete` must be the final element so the
        // SSE close logic + frontend progress-bar both terminate on it.
        assert_eq!(*INSTALL_STEPS.last().unwrap(), "complete");
    }

    #[test]
    fn install_steps_cover_architecture_phase3_section_4_5_sequence() {
        // Matches §4.5 "ClickHouse install flow" steps 1–4 expanded
        // into operator-visible sub-steps. Renumber here if §4.5 changes.
        for expected in [
            "docker_detected",
            "image_pulling",
            "container_starting",
            "provisioning_user",
            "applying_migrations",
            "running_parity_test",
            "complete",
        ] {
            assert!(INSTALL_STEPS.contains(&expected), "missing step {expected}");
        }
    }
}
