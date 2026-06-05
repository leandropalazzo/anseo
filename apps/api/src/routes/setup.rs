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
use sqlx::Row as _;
use ulid::Ulid;

use axum::extract::Path;

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
        .route("/setup/clickhouse/connect", post(post_clickhouse_connect))
        .route("/setup/clickhouse/status", get(get_clickhouse_etl_status))
        .route("/setup/clickhouse/resume", post(post_clickhouse_resume))
        .route("/setup/webhook/test", post(post_webhook_test))
        .route(
            "/setup/api-keys/:provider/revoke",
            post(post_api_key_revoke),
        )
        .route("/setup/api-keys/:provider", post(post_api_key_set))
}

// ---------------------------------------------------------------------
// POST /v1/setup/clickhouse/connect  (Story 15.4)
// ---------------------------------------------------------------------

/// Remote-connect request from the `/setup` ClickHouse form. `password` is
/// used only to probe the endpoint; it is never persisted to `opengeo.yaml`
/// (the privacy rule — secrets stay in env / the managed provider's store).
#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    /// Managed provider preset the operator chose (informational; persisted).
    #[serde(default)]
    pub preset: Option<String>,
    /// Canonical origin URL, e.g. `https://abc.clickhouse.cloud:8443`.
    pub endpoint: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
}

/// Outcome states the frontend renders distinct copy for.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectState {
    Connected,
    InvalidCredentials,
    Unreachable,
    SchemaIncompatible,
    BadRequest,
    PersistFailed,
}

#[derive(Debug, Serialize)]
pub struct ConnectResult {
    pub ok: bool,
    pub state: ConnectState,
    pub message: String,
    /// Echoed back on success so the UI can confirm what was persisted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

async fn post_clickhouse_connect(
    State(_state): State<AppState>,
    Json(req): Json<ConnectRequest>,
) -> (StatusCode, Json<ConnectResult>) {
    // Reject obviously malformed origins before spending a probe budget.
    let endpoint = req.endpoint.trim().trim_end_matches('/').to_string();
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ConnectResult {
                ok: false,
                state: ConnectState::BadRequest,
                message: "endpoint must be an http(s) origin URL".to_string(),
                endpoint: None,
            }),
        );
    }

    let probe =
        probe_remote_clickhouse(&endpoint, req.username.as_deref(), req.password.as_deref()).await;

    match probe {
        ProbeOutcome::Ok => {}
        ProbeOutcome::InvalidCredentials => {
            return (
                StatusCode::OK,
                Json(ConnectResult {
                    ok: false,
                    state: ConnectState::InvalidCredentials,
                    message: "ClickHouse rejected the credentials (HTTP 401/403)".to_string(),
                    endpoint: None,
                }),
            );
        }
        ProbeOutcome::Unreachable(msg) => {
            return (
                StatusCode::OK,
                Json(ConnectResult {
                    ok: false,
                    state: ConnectState::Unreachable,
                    message: format!("could not reach {endpoint}: {msg}"),
                    endpoint: None,
                }),
            );
        }
        ProbeOutcome::SchemaIncompatible(code) => {
            return (
                StatusCode::OK,
                Json(ConnectResult {
                    ok: false,
                    state: ConnectState::SchemaIncompatible,
                    message: format!("endpoint responded but the probe query failed (HTTP {code})"),
                    endpoint: None,
                }),
            );
        }
    }

    // Probe succeeded — persist the endpoint (sans password) to opengeo.yaml.
    match persist_clickhouse_endpoint(&endpoint, &req) {
        Ok(()) => (
            StatusCode::OK,
            Json(ConnectResult {
                ok: true,
                state: ConnectState::Connected,
                message: "connected and saved to opengeo.yaml".to_string(),
                endpoint: Some(endpoint),
            }),
        ),
        Err(msg) => (
            StatusCode::OK,
            Json(ConnectResult {
                ok: false,
                state: ConnectState::PersistFailed,
                message: format!("connected, but failed to persist config: {msg}"),
                endpoint: None,
            }),
        ),
    }
}

enum ProbeOutcome {
    Ok,
    InvalidCredentials,
    Unreachable(String),
    SchemaIncompatible(String),
}

/// Probe a remote ClickHouse over HTTP by running `SELECT 1`. Uses a `curl`
/// subprocess (matching `setup_probe::probe_clickhouse`) so we don't pull
/// reqwest into the API just for setup wiring.
async fn probe_remote_clickhouse(
    endpoint: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> ProbeOutcome {
    let query_url = format!("{endpoint}/?query=SELECT%201");
    let mut cmd = tokio::process::Command::new("curl");
    cmd.args([
        "--max-time",
        "5",
        "-s",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
    ]);
    if let Some(user) = username {
        // Pass credentials via curl `-u user:pass`. Empty password is allowed.
        let cred = format!("{user}:{}", password.unwrap_or(""));
        cmd.arg("-u").arg(cred);
    }
    cmd.arg(&query_url);

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => return ProbeOutcome::Unreachable(format!("curl failed to spawn: {e}")),
    };
    let code = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match code.as_str() {
        "200" => ProbeOutcome::Ok,
        // curl emits "000" when it never got an HTTP response (DNS, refused,
        // timeout) — treat as unreachable.
        "000" | "" => ProbeOutcome::Unreachable("no HTTP response".to_string()),
        "401" | "403" => ProbeOutcome::InvalidCredentials,
        other => ProbeOutcome::SchemaIncompatible(other.to_string()),
    }
}

/// Load `opengeo.yaml` (path from `OGEO_CONFIG`, default `opengeo.yaml`),
/// set `analytics.clickhouse`, and write it back. The password is never
/// written; it is expected at runtime via `CLICKHOUSE_PASSWORD`.
fn persist_clickhouse_endpoint(endpoint: &str, req: &ConnectRequest) -> Result<(), String> {
    use opengeo_core::config::{AnalyticsConfig, ClickHouseEndpointConfig};

    let path = std::env::var("OGEO_CONFIG").unwrap_or_else(|_| "opengeo.yaml".to_string());
    let mut config = opengeo_core::Config::from_path(&path).map_err(|e| e.to_string())?;

    let ch = ClickHouseEndpointConfig {
        endpoint: endpoint.to_string(),
        database: req.database.clone(),
        username: req.username.clone(),
        preset: req.preset.clone(),
    };
    match config.analytics.as_mut() {
        Some(a) => a.clickhouse = Some(ch),
        None => {
            config.analytics = Some(AnalyticsConfig {
                clickhouse: Some(ch),
            })
        }
    }

    let yaml = config.to_yaml_string().map_err(|e| e.to_string())?;
    std::fs::write(&path, yaml).map_err(|e| format!("write {path}: {e}"))?;
    Ok(())
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
                    let payload =
                        serde_json::to_string(&first).unwrap_or_else(|_| "{}".to_string());
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

// ---------------------------------------------------------------------
// GET /v1/setup/clickhouse/status  (Story 30-8b)
// ---------------------------------------------------------------------
//
// Surfaces the resumable-ETL checkpoint row (`analytics_migration_state`,
// migration 20260530110000) so the `/setup` UI can render migration
// progress and offer a resume. We read the row with a RUNTIME
// `sqlx::query_as` (NOT the `query!` macro) so the offline `.sqlx` cache
// stays untouched — this matches the binding discipline in the read
// routes (`brands.rs`, `citations.rs`, ...).
//
// `project_id` is a `ProjectId` (a ULID newtype) but encodes/decodes as a
// Postgres `UUID` (see the `sqlx::Type`/`Encode` impls on `ulid_newtype!`
// in `opengeo-core::ids`), so we bind `state.project_id` directly against
// the `project_id UUID PRIMARY KEY` column.

/// A heartbeat older than this (or NULL) on a not-yet-finished row means
/// the ETL process is no longer running — the run is `interrupted` and
/// resumable. The engine bumps `last_heartbeat_at` once per committed
/// batch, so any healthy run refreshes it well inside this window.
const HEARTBEAT_STALE_SECS: i64 = 60;

/// Raw projection of the `analytics_migration_state` columns this endpoint
/// reads. Order/names match migration `20260530110000_analytics_migration_state.sql`.
#[derive(Debug, sqlx::FromRow)]
struct MigrationStateRow {
    last_completed_batch_id: i64,
    batch_size: i32,
    total_rows_estimate: i64,
    last_heartbeat_at: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

/// Wire shape consumed by the `/setup` ETL section (frontend Story 30-8f).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct EtlStatus {
    /// `idle` (no row) | `running` | `interrupted` | `completed` | `unknown`.
    pub state: String,
    /// Checkpoint value: batches committed so far (`last_completed_batch_id`).
    pub batches_done: Option<i64>,
    /// `ceil(total_rows_estimate / batch_size)`, when both are known/positive.
    pub batches_total: Option<i64>,
    pub last_heartbeat_at: Option<String>,
    pub finished_at: Option<String>,
    /// Populated only on the `unknown` state (query failure); the message
    /// is operator-facing diagnostic text.
    pub error: Option<String>,
}

async fn get_clickhouse_etl_status(
    project: crate::extractors::ProjectScope,
    State(state): State<AppState>,
) -> Json<EtlStatus> {
    let row = sqlx::query_as::<_, MigrationStateRow>(
        "SELECT last_completed_batch_id, batch_size, total_rows_estimate, \
                last_heartbeat_at, started_at, finished_at \
         FROM analytics_migration_state WHERE project_id = $1",
    )
    .bind(project.id())
    .fetch_optional(state.storage.pool())
    .await;

    let row = match row {
        Ok(r) => r,
        Err(e) => {
            // Query failed (table missing, pool down) — report `unknown`
            // rather than 500 so the UI degrades gracefully, mirroring the
            // per-section `unknown` contract of `GET /v1/setup/status`.
            return Json(EtlStatus {
                state: "unknown".to_string(),
                batches_done: None,
                batches_total: None,
                last_heartbeat_at: None,
                finished_at: None,
                error: Some(e.to_string()),
            });
        }
    };

    let Some(row) = row else {
        // No checkpoint row → migration has never run for this project.
        return Json(EtlStatus {
            state: "idle".to_string(),
            batches_done: None,
            batches_total: None,
            last_heartbeat_at: None,
            finished_at: None,
            error: None,
        });
    };

    // batches_total = ceil(total_rows_estimate / batch_size), guarding the
    // (defensive) batch_size <= 0 / estimate <= 0 cases.
    let batches_total = if row.batch_size > 0 && row.total_rows_estimate > 0 {
        let bs = row.batch_size as i64;
        Some((row.total_rows_estimate + bs - 1) / bs)
    } else {
        None
    };

    // State derivation:
    //   finished_at set                          → completed
    //   recent heartbeat                         → running
    //   NULL / stale heartbeat & not finished    → interrupted
    let state_str = if row.finished_at.is_some() {
        "completed"
    } else {
        match row.last_heartbeat_at {
            Some(hb) if (Utc::now() - hb).num_seconds() <= HEARTBEAT_STALE_SECS => "running",
            _ => "interrupted",
        }
    };

    Json(EtlStatus {
        state: state_str.to_string(),
        batches_done: Some(row.last_completed_batch_id),
        batches_total,
        last_heartbeat_at: row.last_heartbeat_at.map(|t| t.to_rfc3339()),
        finished_at: row.finished_at.map(|t| t.to_rfc3339()),
        error: None,
    })
}

// ---------------------------------------------------------------------
// POST /v1/setup/clickhouse/resume  (Story 30-8b)
// ---------------------------------------------------------------------
//
// IMPLEMENTATION NOTE — Story 31-5 hardening: enqueue a real worker job.
// The resumable engine `opengeo_analytics::metrics_store::clickhouse_etl::
// migrate_project_resumable` (and the `ClickHouseMetricsStore` it needs)
// live entirely behind the analytics crate's `clickhouse` Cargo feature,
// which `opengeo-api` does NOT enable — so the symbol is not even linkable
// from this handler. The WORKER (`apps/worker`, built `--features clickhouse`)
// owns ETL execution; the API only enqueues. Story 31-4 created the
// `etl_jobs` queue table for exactly this seam: we INSERT a `pending` row and
// the worker claims it at-most-once on its next poll, runs the resumable
// migration (resuming from `analytics_migration_state.last_completed_batch_id`
// when a checkpoint exists), and records terminal state.
//
// We do the INSERT with a direct runtime `sqlx::query` here (rather than
// calling `opengeo_worker::etl::enqueue_etl_job`) so the API does NOT take a
// dependency on the worker crate. The columns/semantics mirror that helper:
// `id`/`requested_at` default in-DB (gen_random_uuid()/now()), so we only bind
// `project_id`; `status` defaults to 'pending' but we set it explicitly for
// clarity. `project_id` is bound as its canonical UUID form (the `etl_jobs`
// column is `UUID NOT NULL REFERENCES projects(id)`), via
// `uuid::Uuid::from_bytes(project.id().into_ulid().to_bytes())`.

#[derive(Debug, Serialize)]
pub struct ResumeAccepted {
    /// True when a worker ETL job was successfully enqueued for this project.
    pub triggered: bool,
    /// The enqueued job's id (UUID), when the INSERT succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub message: String,
}

async fn post_clickhouse_resume(
    project: crate::extractors::ProjectScope,
    State(state): State<AppState>,
) -> (StatusCode, Json<ResumeAccepted>) {
    let pool = state.storage.pool();

    // The `etl_jobs.project_id` column is a Postgres UUID; convert the ULID
    // newtype to its canonical UUID byte form so the FK to `projects(id)`
    // matches the row the worker will resolve back to a `ProjectId`.
    let project_uuid = uuid::Uuid::from_bytes(project.id().into_ulid().to_bytes());

    // Inline enqueue — mirrors `opengeo_worker::etl::enqueue_etl_job` but
    // without pulling in the worker crate. `id` and `requested_at` use their
    // in-DB defaults; we RETURNING the generated id for the response.
    let result = sqlx::query(
        "INSERT INTO etl_jobs (project_id, status) VALUES ($1, 'pending') RETURNING id",
    )
    .bind(project_uuid)
    .fetch_one(pool)
    .await;

    match result {
        Ok(row) => {
            let job_id: uuid::Uuid = match row.try_get("id") {
                Ok(id) => id,
                Err(e) => {
                    return (
                        StatusCode::ACCEPTED,
                        Json(ResumeAccepted {
                            triggered: true,
                            job_id: None,
                            message: format!("ETL job enqueued, but failed to read its id: {e}"),
                        }),
                    );
                }
            };
            (
                StatusCode::ACCEPTED,
                Json(ResumeAccepted {
                    triggered: true,
                    job_id: Some(job_id.to_string()),
                    message: "ETL migration enqueued; the worker will resume from the last \
                              completed batch on its next poll"
                        .to_string(),
                }),
            )
        }
        Err(e) => (
            StatusCode::ACCEPTED,
            Json(ResumeAccepted {
                triggered: false,
                job_id: None,
                message: format!("failed to enqueue ETL job: {e}"),
            }),
        ),
    }
}

// ---------------------------------------------------------------------
// POST /v1/setup/webhook/test  (Story 30-9, hardened in Story 31-5)
// ---------------------------------------------------------------------
//
// Fires a single signed test POST at the operator-supplied URL using the
// SAME pieces the real webhook dispatcher uses:
//
//   * The target's ACTUAL per-webhook secret: we look up the registered
//     `webhooks` row whose `target_url` matches `url`, then decrypt its
//     `secret_ciphertext` exactly the way the dispatcher's poller does
//     (RFC 4648 standard base64 — see
//     `crates/scheduler/src/webhooks/poller.rs::decode_secret`).
//   * The real HMAC signer
//     (`opengeo_scheduler::webhooks::signer::{sign, SIGNATURE_HEADER}`).
//   * `reqwest` for transport — the same client `deliver_one` uses
//     (`crates/scheduler/src/webhooks/dispatcher.rs`).
//
// So a delivery from this handler is byte-for-byte what a live dispatch
// would send to that target, and the consumer's signature verification
// (against the secret it shares with us) succeeds.
//
// `signature_valid` reports whether the `X-OpenGEO-Signature` header we
// attached round-trips through the real `verify()` against the body+secret
// we actually used — i.e. a genuine OpenGEO signature the target's secret
// will accept. It is `null` when we never got far enough to sign (bad URL,
// or no registered webhook matches `url` — in which case we have no secret
// and refuse to fabricate one).

#[derive(Debug, Deserialize)]
pub struct WebhookTestRequest {
    pub url: String,
}

/// Wire shape consumed by the `/setup` webhook section (frontend stub
/// `postWebhookTest` in `apps/web/lib/api/setup.ts`). All fields nullable so
/// the UI can render partial outcomes (e.g. signed-but-unreachable).
#[derive(Debug, Serialize)]
pub struct WebhookTestResult {
    /// HTTP status the target returned, or `null` if we never got a response.
    pub status_code: Option<i64>,
    /// `true` when the signature we attached round-trips through the real
    /// `verify()`; `null` when we never signed (e.g. bad URL, no match).
    pub signature_valid: Option<bool>,
    /// Wall-clock round-trip in milliseconds, or `null` on no response.
    pub latency_ms: Option<i64>,
    /// Operator-facing diagnostic; `null` on success.
    pub error: Option<String>,
}

/// Normalize a URL for matching a registered webhook against the requested
/// test target: trim whitespace and a single trailing slash so
/// `https://h/hook` and `https://h/hook/` compare equal.
fn normalize_webhook_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

/// Decode a stored `secret_ciphertext` (RFC 4648 standard base64, padding
/// tolerant) back to raw secret bytes. This is the inverse of the CLI's
/// `base64_encode` and mirrors `poller.rs::decode_secret` exactly so the
/// secret we sign with is identical to the one the dispatcher would use.
/// Returns `None` on malformed input (operator manually inserted garbage).
fn decode_webhook_secret(ciphertext: &str) -> Option<Vec<u8>> {
    const fn build_reverse() -> [i16; 128] {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut table = [-1i16; 128];
        let mut i = 0;
        while i < alphabet.len() {
            table[alphabet[i] as usize] = i as i16;
            i += 1;
        }
        table
    }
    const REVERSE: [i16; 128] = build_reverse();

    let bytes = ciphertext.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
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
        out.push(n0 as u8);
        if padding < 2 {
            let n1 = (((buf[1] as u32) & 0x0f) << 4) | ((buf[2] as u32) >> 2);
            out.push(n1 as u8);
        }
        if padding < 1 {
            let n2 = (((buf[2] as u32) & 0x03) << 6) | (buf[3] as u32);
            out.push(n2 as u8);
        }
    }
    Some(out)
}

async fn post_webhook_test(
    project: crate::extractors::ProjectScope,
    State(state): State<AppState>,
    Json(req): Json<WebhookTestRequest>,
) -> (StatusCode, Json<WebhookTestResult>) {
    use opengeo_scheduler::webhooks::signer::{
        sign, verify, DEFAULT_REPLAY_WINDOW_SECONDS, SIGNATURE_HEADER,
    };

    let url = normalize_webhook_url(&req.url);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return (
            StatusCode::BAD_REQUEST,
            Json(WebhookTestResult {
                status_code: None,
                signature_valid: None,
                latency_ms: None,
                error: Some("url must be an http(s) URL".to_string()),
            }),
        );
    }

    // Look up the registered webhook whose target matches `url`. We refuse
    // to fabricate a secret for an unregistered URL — without a shared
    // secret a "test" delivery proves nothing the consumer can verify.
    let webhooks = match state
        .storage
        .webhooks()
        .list_for_project(project.id())
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            return (
                StatusCode::OK,
                Json(WebhookTestResult {
                    status_code: None,
                    signature_valid: None,
                    latency_ms: None,
                    error: Some(format!("failed to load webhooks: {e}")),
                }),
            );
        }
    };

    let Some(webhook) = webhooks
        .into_iter()
        .find(|w| normalize_webhook_url(&w.target_url) == url)
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(WebhookTestResult {
                status_code: None,
                signature_valid: None,
                latency_ms: None,
                error: Some(format!(
                    "no registered webhook targets `{url}` for this project; \
                     declare it with `ogeo webhook add` first"
                )),
            }),
        );
    };

    // Decrypt the per-webhook secret the SAME way the dispatcher does.
    let Some(secret) = decode_webhook_secret(&webhook.secret_ciphertext) else {
        return (
            StatusCode::OK,
            Json(WebhookTestResult {
                status_code: None,
                signature_valid: None,
                latency_ms: None,
                error: Some(
                    "stored webhook secret is malformed (not valid base64); \
                     rotate it with `ogeo webhook rotate-secret`"
                        .to_string(),
                ),
            }),
        );
    };

    // Canonical test payload — same JSON content-type + `event_kind` shape a
    // real delivery carries, so a consumer's parser exercises the live path.
    let timestamp_unix = Utc::now().timestamp();
    let body = serde_json::to_vec(&serde_json::json!({
        "event_kind": "setup.webhook_test",
        "sent_at": Utc::now().to_rfc3339(),
    }))
    .unwrap_or_else(|_| b"{}".to_vec());

    // Sign with the SAME signer + the target's REAL secret.
    let signature = sign(&secret, &body, timestamp_unix);

    // Self-verify the header we're about to send (round-trips through the
    // real `verify()` against the same secret), so `signature_valid`
    // reflects a genuinely-valid OpenGEO signature the target will accept.
    let signature_valid = verify(
        &secret,
        &body,
        Some(&signature),
        timestamp_unix,
        DEFAULT_REPLAY_WINDOW_SECONDS,
    )
    .is_ok();

    // POST via reqwest — the same transport `deliver_one` uses.
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::OK,
                Json(WebhookTestResult {
                    status_code: None,
                    signature_valid: Some(signature_valid),
                    latency_ms: None,
                    error: Some(format!("failed to build HTTP client: {e}")),
                }),
            );
        }
    };

    let started = std::time::Instant::now();
    let send_result = client
        .post(&url)
        .header(SIGNATURE_HEADER, &signature)
        .header("Content-Type", "application/json")
        .body(body.clone())
        .send()
        .await;
    let latency_ms = started.elapsed().as_millis() as i64;

    match send_result {
        Ok(response) => (
            StatusCode::OK,
            Json(WebhookTestResult {
                status_code: Some(response.status().as_u16() as i64),
                signature_valid: Some(signature_valid),
                latency_ms: Some(latency_ms),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(WebhookTestResult {
                status_code: None,
                signature_valid: Some(signature_valid),
                latency_ms: Some(latency_ms),
                error: Some(format!("no HTTP response from {url}: {e}")),
            }),
        ),
    }
}

// ---------------------------------------------------------------------
// POST /v1/setup/api-keys/:provider/revoke  (Story 30-9)
// ---------------------------------------------------------------------
//
// Revokes a stored provider API key from the SAME secret-store chain the
// CLI `ogeo login` writes to and `setup_probe::probe_api_keys` reads from
// (`opengeo_core::default_chain()` → keyring → age-file → in-memory).
//
// Story 31-5 hardening: this is now a REAL delete. We call the
// `SecretStore::remove` method (added in 31-6) on the `default_chain()` store,
// which drops the provider's entry from every leg of the chain. After a
// successful revoke a subsequent `get` for the same provider returns
// `NotFound` — the key is genuinely gone, not overwritten with an empty
// value. `remove` is idempotent: revoking a provider that was never
// configured is a no-op success, matching `ApiKeyRepo::revoke`'s contract.

#[derive(Debug, Serialize)]
pub struct ApiKeyRevokeResult {
    pub revoked: bool,
    pub provider: String,
    pub message: String,
}

async fn post_api_key_revoke(
    State(_state): State<AppState>,
    Path(provider): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    use opengeo_core::{ProviderName, SecretStore as _};

    // Validate against the known first-party provider wire names. Reject
    // anything else with a 400 rather than deleting an arbitrary string.
    let Some(parsed) = ProviderName::parse(&provider) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unknown_provider",
                "message": format!(
                    "unsupported provider `{provider}`; expected one of {}",
                    ProviderName::all_wire_names().join(", ")
                ),
            })),
        );
    };
    let wire = parsed.as_wire_str().to_string();

    // The keyring/age-file backends can block (macOS keychain prompt, file
    // IO), so run the store interaction on a blocking thread — mirroring
    // `probe_api_keys`.
    let wire_for_task = wire.clone();
    let join = tokio::task::spawn_blocking(move || {
        let store = opengeo_core::default_chain();
        // Hard delete across every leg of the chain. After this returns Ok,
        // `get` resolves NotFound.
        store.remove(&wire_for_task)
    })
    .await;

    match join {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "revoked": true,
                "provider": wire,
                "message": "provider key revoked",
            })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "secret_store_error",
                "provider": wire,
                "message": e.to_string(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "revoke_task_failed",
                "provider": wire,
                "message": e.to_string(),
            })),
        ),
    }
}

// ---------------------------------------------------------------------
// POST /v1/setup/api-keys/:provider   (set / connect a provider key)
// ---------------------------------------------------------------------
//
// Stores a provider API key in the SAME secret-store chain the CLI
// `ogeo login` writes to and `setup_probe::probe_api_keys` reads from
// (`opengeo_core::default_chain()` → keyring → age-file → in-memory). On a
// headless container the keyring leg is unavailable, so the write lands in
// the age-file leg — which is exactly what the live provider registry reads
// when building real clients. The key value is never echoed back in the
// response (only `configured: true`), mirroring `probe_api_keys`.

#[derive(Debug, Deserialize)]
pub struct ApiKeySetRequest {
    pub key: String,
}

async fn post_api_key_set(
    State(_state): State<AppState>,
    Path(provider): Path<String>,
    Json(body): Json<ApiKeySetRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    use opengeo_core::{ProviderName, Secret, SecretStore as _};

    let Some(parsed) = ProviderName::parse(&provider) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unknown_provider",
                "message": format!(
                    "unsupported provider `{provider}`; expected one of {}",
                    ProviderName::all_wire_names().join(", ")
                ),
            })),
        );
    };
    // Plugin providers source credentials from their own loader, not this
    // first-party chain — reject rather than store an orphan secret.
    if parsed.is_plugin() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unsupported_provider",
                "message": "plugin providers manage their own credentials",
            })),
        );
    }
    let key = body.key.trim().to_string();
    if key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "empty_key",
                "message": "API key must not be empty",
            })),
        );
    }
    let wire = parsed.as_wire_str().to_string();

    // Keyring/age-file backends can block (keychain prompt, file IO), so run
    // the store write on a blocking thread — mirroring `post_api_key_revoke`.
    let wire_for_task = wire.clone();
    let join = tokio::task::spawn_blocking(move || {
        let store = opengeo_core::default_chain();
        store.set(&wire_for_task, Secret::new(key))
    })
    .await;

    match join {
        Ok(Ok(())) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "configured": true,
                "provider": wire,
                "message": "provider key stored",
            })),
        ),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "secret_store_error",
                "provider": wire,
                "message": e.to_string(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "set_task_failed",
                "provider": wire,
                "message": e.to_string(),
            })),
        ),
    }
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

    // -----------------------------------------------------------------
    // GET /v1/setup/clickhouse/status — state-derivation (Story 30-8b)
    // -----------------------------------------------------------------

    use std::sync::Arc;

    use opengeo_core::ProjectId;

    /// Serializes tests that mutate the process-global `XDG_CONFIG_HOME` /
    /// `AGE_PASSPHRASE_ENV` env vars to steer the secret-store chain. Without
    /// this, parallel test threads clobber each other's age-file path and
    /// passphrase. Async-aware so the guard can be held across `.await`.
    static SECRET_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Minimal `AppState` over a test pool — enough for the ETL-status
    /// handler, which only touches `storage.pool()` and `project_id`.
    fn state_for(pool: sqlx::PgPool, project_id: ProjectId) -> AppState {
        let storage = Arc::new(opengeo_storage::Storage::from_pool(pool));
        let (events, _rx) = opengeo_scheduler::worker::event_channel();
        AppState {
            storage,
            project_id,
            events,
            config: None,
            provider_registry: None,
            configured_project: Arc::new("default".to_string()),
            setup_install_state: Arc::new(tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            serve_info: None,
        }
    }

    /// A resolved project scope for handlers that now take `ProjectScope`
    /// directly (the `/v1`-only setup handlers). Mirrors what the project
    /// header guard would stamp.
    fn scope_for(project_id: ProjectId) -> crate::extractors::ProjectScope {
        crate::extractors::ProjectScope {
            id: project_id,
            name: "default".to_string(),
        }
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn etl_status_idle_when_no_row(pool: sqlx::PgPool) {
        let project_id = ProjectId::new();
        let state = state_for(pool, project_id);

        let Json(status) = get_clickhouse_etl_status(scope_for(project_id), State(state)).await;
        assert_eq!(status.state, "idle");
        assert_eq!(status.batches_done, None);
        assert_eq!(status.batches_total, None);
        assert_eq!(status.finished_at, None);
        assert_eq!(status.error, None);
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn etl_status_completed_when_finished_row(pool: sqlx::PgPool) {
        let project_id = ProjectId::new();

        // Seed a finished checkpoint: 250 rows / batch_size 100 = 3 batches.
        sqlx::query(
            "INSERT INTO analytics_migration_state \
                 (project_id, last_completed_batch_id, batch_size, \
                  total_rows_estimate, last_heartbeat_at, started_at, finished_at) \
             VALUES ($1, 3, 100, 250, now(), now(), now())",
        )
        .bind(project_id)
        .execute(&pool)
        .await
        .expect("seed finished migration row");

        let state = state_for(pool, project_id);
        let Json(status) = get_clickhouse_etl_status(scope_for(project_id), State(state)).await;

        assert_eq!(status.state, "completed");
        assert_eq!(status.batches_done, Some(3));
        // ceil(250 / 100) == 3.
        assert_eq!(status.batches_total, Some(3));
        assert!(status.finished_at.is_some());
        assert_eq!(status.error, None);
    }

    // -----------------------------------------------------------------
    // POST /v1/setup/webhook/test  (Story 30-9)
    // -----------------------------------------------------------------

    /// Standard RFC 4648 base64 (the inverse of `decode_webhook_secret`), used
    /// only to seed a registered webhook's `secret_ciphertext` in tests.
    fn base64_encode(input: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[((n >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3f) as usize] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[((n >> 6) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[(n & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }

    /// Seed a `projects` row so a webhook (FK to projects) can be inserted.
    async fn seed_project(pool: &sqlx::PgPool, project_id: ProjectId) {
        let project_uuid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
        sqlx::query(
            "INSERT INTO projects (id, name, organization_id, tenant_id, created_at) \
             VALUES ($1, $2, NULL, NULL, now())",
        )
        .bind(project_uuid)
        .bind(format!("proj-{}", &project_uuid.to_string()[..8]))
        .execute(pool)
        .await
        .expect("seed project row");
    }

    /// Spins up a one-shot TCP server that returns 200 and captures the raw
    /// request bytes so the test can assert the `X-OpenGEO-Signature` header
    /// (produced by the real signer, using the REGISTERED webhook's decrypted
    /// secret) round-trips through `verify()`.
    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn webhook_test_signs_and_reports_status(pool: sqlx::PgPool) {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let target_url = format!("http://{addr}/hook");

        // Register a webhook for this project with a known secret, so the
        // hardened handler can look it up and sign with its REAL secret.
        let project_id = ProjectId::new();
        seed_project(&pool, project_id).await;
        let secret_ciphertext = base64_encode(b"super-secret-webhook-key");
        let storage = opengeo_storage::Storage::from_pool(pool.clone());
        storage
            .webhooks()
            .insert(
                project_id,
                "test-hook",
                &target_url,
                &secret_ciphertext,
                &serde_json::json!(["setup.webhook_test"]),
            )
            .await
            .expect("register webhook");

        // Accept exactly one connection, read the request, reply 200.
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = sock.read(&mut buf).await.unwrap();
            let raw = String::from_utf8_lossy(&buf[..n]).to_string();
            let _ = sock
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await;
            let _ = sock.flush().await;
            raw
        });

        let state = state_for(pool, project_id);
        let (code, Json(result)) = post_webhook_test(
            scope_for(project_id),
            State(state),
            Json(WebhookTestRequest {
                url: target_url.clone(),
            }),
        )
        .await;

        assert_eq!(code, StatusCode::OK, "error: {:?}", result.error);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.signature_valid, Some(true));
        assert!(result.latency_ms.is_some());
        assert_eq!(result.error, None);

        // The request the target actually received must carry a v1 OpenGEO
        // signature header from the real signer.
        let raw = server.await.unwrap();
        let lower = raw.to_lowercase();
        assert!(
            lower.contains("x-opengeo-signature: v1=t="),
            "missing signed header in request:\n{raw}"
        );
    }

    /// An unregistered target URL must be refused (404, no fabricated secret).
    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn webhook_test_unregistered_url_is_refused(pool: sqlx::PgPool) {
        let pid = ProjectId::new();
        let state = state_for(pool, pid);
        let (code, Json(result)) = post_webhook_test(
            scope_for(pid),
            State(state),
            Json(WebhookTestRequest {
                url: "https://unregistered.example.com/hook".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::NOT_FOUND);
        assert_eq!(result.signature_valid, None);
        assert!(result.error.is_some());
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn webhook_test_rejects_non_http_url(pool: sqlx::PgPool) {
        let pid = ProjectId::new();
        let state = state_for(pool, pid);
        let (code, Json(result)) = post_webhook_test(
            scope_for(pid),
            State(state),
            Json(WebhookTestRequest {
                url: "ftp://example.com".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(result.signature_valid, None);
        assert!(result.error.is_some());
    }

    // -----------------------------------------------------------------
    // POST /v1/setup/api-keys/:provider/revoke  (Story 30-9)
    // -----------------------------------------------------------------

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn api_key_revoke_rejects_unknown_provider(pool: sqlx::PgPool) {
        let state = state_for(pool, ProjectId::new());
        let (code, Json(body)) =
            post_api_key_revoke(State(state), Path("not-a-provider".to_string())).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "unknown_provider");
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn api_key_revoke_truly_removes_key(pool: sqlx::PgPool) {
        use opengeo_core::secret_store::{AgeFileStore, SecretStoreError};
        use opengeo_core::{Secret, SecretStore as _};

        let _env_guard = SECRET_ENV_LOCK.lock().await;

        // Steer the secret-store chain at an isolated temp age-file so the
        // test never touches the developer's real keyring entries. The chain
        // still tries keyring first; on a headless CI box that leg errors
        // (no Secret Service / DBus), but `ChainedStore::remove` still calls
        // every leg, so the age-file leg — the only writable one here — has
        // the entry deleted regardless. We verify the real delete against the
        // age-file leg directly so the assertion holds whether or not a live
        // keyring is present.
        let tmp = std::env::temp_dir().join(format!("ogeo-revoke-test-{}", Ulid::new()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &tmp);
        std::env::set_var(opengeo_core::AGE_PASSPHRASE_ENV, "test-revoke-pass");

        // Path `default_chain()` resolves its age-file leg to under our temp
        // XDG_CONFIG_HOME — seed + assert against that exact leg.
        let age_path = tmp.join("opengeo").join("secrets.age");
        let age = AgeFileStore::at(age_path);
        age.set("openai", Secret::new("sk-fixture-to-revoke"))
            .expect("seed provider key in age-file leg");
        assert_eq!(
            age.get("openai").unwrap().expose(),
            "sk-fixture-to-revoke",
            "fixture key should be present before revoke"
        );

        let state = state_for(pool, ProjectId::new());
        let (code, Json(body)) =
            post_api_key_revoke(State(state), Path("openai".to_string())).await;

        // The hard delete must make a subsequent `get` on the writable leg
        // resolve NotFound — the key is genuinely gone, not tombstoned empty.
        let after = age.get("openai");
        assert!(
            matches!(after, Err(SecretStoreError::NotFound { .. })),
            "key should be gone from the age-file leg after revoke, got: {after:?}"
        );

        // HTTP outcome: 200 + revoked:true where every chain leg is
        // operable; on a headless box the keyring leg surfaces a backend
        // error so the chain reports 500. Both are structured, non-panicking
        // outcomes carrying the provider; assert the provider either way and
        // that the real delete (above) happened regardless.
        assert!(
            code == StatusCode::OK || code == StatusCode::INTERNAL_SERVER_ERROR,
            "unexpected status {code}: {body}"
        );
        assert_eq!(body["provider"], "openai");
        if code == StatusCode::OK {
            assert_eq!(body["revoked"], true);
        }

        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var(opengeo_core::AGE_PASSPHRASE_ENV);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -----------------------------------------------------------------
    // POST /v1/setup/api-keys/:provider  (set / connect)
    // -----------------------------------------------------------------

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn api_key_set_rejects_unknown_provider(pool: sqlx::PgPool) {
        let state = state_for(pool, ProjectId::new());
        let (code, Json(body)) = post_api_key_set(
            State(state),
            Path("not-a-provider".to_string()),
            Json(ApiKeySetRequest {
                key: "sk-whatever".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "unknown_provider");
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn api_key_set_rejects_empty_key(pool: sqlx::PgPool) {
        let state = state_for(pool, ProjectId::new());
        let (code, Json(body)) = post_api_key_set(
            State(state),
            Path("openai".to_string()),
            Json(ApiKeySetRequest {
                key: "   ".to_string(),
            }),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "empty_key");
    }

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn api_key_set_stores_key_in_chain(pool: sqlx::PgPool) {
        use opengeo_core::SecretStore as _;

        let _env_guard = SECRET_ENV_LOCK.lock().await;

        // Isolate the secret-store chain at a temp age-file (mirrors the
        // revoke test) so we never touch the developer's real keyring.
        let tmp = std::env::temp_dir().join(format!("ogeo-set-test-{}", Ulid::new()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &tmp);
        std::env::set_var(opengeo_core::AGE_PASSPHRASE_ENV, "test-set-pass");

        let state = state_for(pool, ProjectId::new());
        let (code, Json(body)) = post_api_key_set(
            State(state),
            Path("openai".to_string()),
            Json(ApiKeySetRequest {
                key: "sk-fixture-stored".to_string(),
            }),
        )
        .await;

        assert_eq!(code, StatusCode::OK, "unexpected status: {body}");
        assert_eq!(body["provider"], "openai");
        assert_eq!(body["configured"], true);
        // Response must never echo the key value back.
        assert!(
            !body.to_string().contains("sk-fixture-stored"),
            "response leaked the key value: {body}"
        );

        // The stored key must be resolvable through the same chain the live
        // provider registry reads.
        let got = opengeo_core::default_chain().get("openai");
        assert_eq!(
            got.unwrap().expose(),
            "sk-fixture-stored",
            "key should be retrievable from the chain after set"
        );

        // Clean up so a real keyring (dev box) isn't left polluted.
        let _ = opengeo_core::default_chain().remove("openai");
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var(opengeo_core::AGE_PASSPHRASE_ENV);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // -----------------------------------------------------------------
    // POST /v1/setup/clickhouse/resume  (Story 31-5 — real enqueue)
    // -----------------------------------------------------------------

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn resume_enqueues_pending_etl_job(pool: sqlx::PgPool) {
        let project_id = ProjectId::new();
        let project_uuid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());

        // `etl_jobs.project_id` FKs `projects(id)`, so the project row must
        // exist before the handler's INSERT can succeed.
        sqlx::query(
            "INSERT INTO projects (id, name, organization_id, tenant_id, created_at) \
             VALUES ($1, $2, NULL, NULL, now())",
        )
        .bind(project_uuid)
        .bind("resume-test")
        .execute(&pool)
        .await
        .expect("seed project row");

        let state = state_for(pool.clone(), project_id);
        let (code, Json(resp)) = post_clickhouse_resume(scope_for(project_id), State(state)).await;

        assert_eq!(code, StatusCode::ACCEPTED);
        assert!(resp.triggered, "resume should enqueue: {}", resp.message);
        let job_id = resp.job_id.expect("enqueued job id");

        // The enqueued row must exist, be `pending`, and target this project.
        let row = sqlx::query("SELECT project_id, status FROM etl_jobs WHERE id = $1")
            .bind(uuid::Uuid::parse_str(&job_id).unwrap())
            .fetch_one(&pool)
            .await
            .expect("enqueued etl_jobs row exists");
        let row_project: uuid::Uuid = row.try_get("project_id").unwrap();
        let status: String = row.try_get("status").unwrap();
        assert_eq!(row_project, project_uuid);
        assert_eq!(status, "pending");
    }
}
