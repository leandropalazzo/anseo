#![allow(clippy::doc_overindented_list_items)]
//! Story 15.1 — Synchronous probe functions for `GET /v1/setup/status`.
//!
//! Each probe is "best effort": it returns a `Section` whose `state`
//! field is `"healthy"`, `"degraded"`, `"not_configured"`, `"running"`,
//! `"stopped"`, or `"unknown"`. On any I/O failure or timeout the probe
//! returns `state: "unknown"` with an `error` string. The `/status`
//! handler aggregates these and always returns HTTP 200 — a single
//! probe failure must never tank the whole status view.
//!
//! All probes are bounded by an `await_with_timeout(Duration)` helper.
//! Default per-probe budgets (per the story spec):
//!   - Postgres: 1s
//!   - ClickHouse: 1s
//!   - Worker: 1s
//!   - Webhook target: 1s
//!   - API keys: 1s
//!   - Docker:  500ms
//!
//! Real Docker / ClickHouse calls land in Story 15.3; for now we shell
//! out to `docker version` and ping the optional `CLICKHOUSE_URL` env
//! over HTTP. The shape returned here is the source-of-truth contract
//! the OpenAPI spec mirrors.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::time::timeout;

use crate::AppState;

const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const DOCKER_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// Top-level `/v1/setup/status` response. Every field is required so the
/// frontend can render skeletons without conditional checks.
#[derive(Debug, Serialize)]
pub struct SetupStatus {
    pub postgres: PostgresSection,
    pub clickhouse: ClickHouseSection,
    pub worker: WorkerSection,
    pub webhook_target: WebhookSection,
    pub api_keys: Vec<ApiKeyEntry>,
    pub docker: DockerSection,
}

#[derive(Debug, Serialize)]
pub struct PostgresSection {
    /// `"healthy" | "degraded" | "unknown"`.
    pub state: String,
    pub schema_version: Option<i64>,
    pub row_count_estimate: Option<i64>,
    pub last_write_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ClickHouseSection {
    /// `"healthy" | "degraded" | "not_configured" | "unknown"`.
    pub state: String,
    pub url: Option<String>,
    pub row_count: Option<i64>,
    pub etl_lag_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkerSection {
    /// `"running" | "stopped" | "unknown"`.
    pub state: String,
    pub uptime_seconds: Option<i64>,
    pub queue_depth: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookSection {
    pub configured: bool,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyEntry {
    pub provider: String,
    pub configured: bool,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct DockerSection {
    pub present: bool,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregate probe — runs all sections in parallel within the per-probe
/// timeout budgets, then assembles the response.
pub async fn probe_all(state: &AppState) -> SetupStatus {
    let (postgres, clickhouse, worker, webhook_target, api_keys, docker) = tokio::join!(
        probe_postgres(state),
        probe_clickhouse(),
        probe_worker(state),
        probe_webhook_target(state),
        probe_api_keys(),
        probe_docker(),
    );
    SetupStatus {
        postgres,
        clickhouse,
        worker,
        webhook_target,
        api_keys,
        docker,
    }
}

/// Postgres section. Reads `_sqlx_migrations` for `schema_version`,
/// `prompt_runs` reltuples for `row_count_estimate`, and `MAX(created_at)`
/// for `last_write_at`. Bounded at `DEFAULT_PROBE_TIMEOUT`.
pub async fn probe_postgres(state: &AppState) -> PostgresSection {
    let unknown = |err: String| PostgresSection {
        state: "unknown".to_string(),
        schema_version: None,
        row_count_estimate: None,
        last_write_at: None,
        error: Some(err),
    };
    let pool = state.storage.pool();
    let fut = async {
        // Latest applied migration version (sqlx migrator stores i64).
        // `MAX(version)` is itself nullable (empty table), so `fetch_one`
        // yields `Option<i64>`.
        let schema_version: Option<i64> =
            sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(version) FROM _sqlx_migrations")
                .fetch_one(pool)
                .await?;
        // Cheap estimate from pg_class — exact COUNT(*) is too expensive
        // on a multi-million-row prompt_runs table.
        let row_count_estimate: Option<i64> = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(reltuples::BIGINT, 0) FROM pg_class WHERE relname = 'prompt_runs'",
        )
        .fetch_optional(pool)
        .await?;
        let last_write_at: Option<DateTime<Utc>> = sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
            "SELECT MAX(created_at) FROM prompt_runs",
        )
        .fetch_one(pool)
        .await?;
        Ok::<_, sqlx::Error>((schema_version, row_count_estimate, last_write_at))
    };
    match timeout(DEFAULT_PROBE_TIMEOUT, fut).await {
        Ok(Ok((schema_version, row_count_estimate, last_write_at))) => PostgresSection {
            state: "healthy".to_string(),
            schema_version,
            row_count_estimate,
            last_write_at,
            error: None,
        },
        Ok(Err(e)) => unknown(format!("postgres probe failed: {e}")),
        Err(_) => unknown(format!(
            "postgres probe timed out after {}ms",
            DEFAULT_PROBE_TIMEOUT.as_millis()
        )),
    }
}

/// ClickHouse section. If `CLICKHOUSE_URL` is unset → `not_configured`
/// (the happy path before story 15.3 installs anything). If set, we ping
/// `<url>/ping` with a 1s budget.
pub async fn probe_clickhouse() -> ClickHouseSection {
    let url = match std::env::var("CLICKHOUSE_URL") {
        Ok(u) if !u.trim().is_empty() => u,
        _ => {
            return ClickHouseSection {
                state: "not_configured".to_string(),
                url: None,
                row_count: None,
                etl_lag_seconds: None,
                error: None,
            };
        }
    };
    let ping_url = format!("{}/ping", url.trim_end_matches('/'));
    let fut = async {
        // We avoid pulling reqwest just for this probe; the stdlib + a
        // raw TCP open-check would be too coarse (no HTTP semantics).
        // Story 15.3 will land the real client; for now we shell out to
        // `curl --max-time 1 -s -o /dev/null -w "%{http_code}"` so the
        // probe surfaces a meaningful HTTP status without adding deps.
        let output = tokio::process::Command::new("curl")
            .args(["--max-time", "1", "-s", "-o", "/dev/null", "-w", "%{http_code}", &ping_url])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let code = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok::<_, String>(code)
    };
    match timeout(DEFAULT_PROBE_TIMEOUT, fut).await {
        Ok(Ok(code)) if code == "200" => ClickHouseSection {
            state: "healthy".to_string(),
            url: Some(url),
            // Real row_count + lag come from `analytics_migration_state`
            // in Story 15.5. For 15.1 we only attest reachability.
            row_count: None,
            etl_lag_seconds: None,
            error: None,
        },
        Ok(Ok(code)) => ClickHouseSection {
            state: "degraded".to_string(),
            url: Some(url),
            row_count: None,
            etl_lag_seconds: None,
            error: Some(format!("ping returned HTTP {code}")),
        },
        Ok(Err(e)) => ClickHouseSection {
            state: "unknown".to_string(),
            url: Some(url),
            row_count: None,
            etl_lag_seconds: None,
            error: Some(format!("clickhouse probe failed: {e}")),
        },
        Err(_) => ClickHouseSection {
            state: "unknown".to_string(),
            url: Some(url),
            row_count: None,
            etl_lag_seconds: None,
            error: Some(format!(
                "clickhouse probe timed out after {}ms",
                DEFAULT_PROBE_TIMEOUT.as_millis()
            )),
        },
    }
}

/// Worker section. Reads queue depth (pending `schedule_ticks`) and
/// derives `state` from "did we see a tick in the last 5 minutes". A
/// proper worker heartbeat lands in a future story; for 15.1 we expose
/// the cheap signal.
pub async fn probe_worker(state: &AppState) -> WorkerSection {
    let unknown = |err: String| WorkerSection {
        state: "unknown".to_string(),
        uptime_seconds: None,
        queue_depth: None,
        error: Some(err),
    };
    let pool = state.storage.pool();
    let fut = async {
        let queue_depth: Option<i64> = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM schedule_ticks WHERE status = 'pending'",
        )
        .fetch_optional(pool)
        .await?;
        let last_tick: Option<DateTime<Utc>> =
            sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
                "SELECT MAX(tick_ts) FROM schedule_ticks",
            )
            .fetch_one(pool)
            .await?;
        Ok::<_, sqlx::Error>((queue_depth, last_tick))
    };
    match timeout(DEFAULT_PROBE_TIMEOUT, fut).await {
        Ok(Ok((queue_depth, last_tick))) => {
            // "running" iff we saw a tick in the last 5 minutes. Otherwise
            // "stopped" (could also mean "no schedules declared yet" —
            // the UI disambiguates via the schedules count).
            let now = Utc::now();
            let running = last_tick
                .map(|t| (now - t).num_seconds() < 300)
                .unwrap_or(false);
            WorkerSection {
                state: if running { "running" } else { "stopped" }.to_string(),
                uptime_seconds: last_tick.map(|t| (now - t).num_seconds().max(0)),
                queue_depth,
                error: None,
            }
        }
        Ok(Err(e)) => unknown(format!("worker probe failed: {e}")),
        Err(_) => unknown(format!(
            "worker probe timed out after {}ms",
            DEFAULT_PROBE_TIMEOUT.as_millis()
        )),
    }
}

/// Webhook target section. Reads the most recent row from
/// `webhook_deliveries` to surface the last delivery timestamp + status.
pub async fn probe_webhook_target(state: &AppState) -> WebhookSection {
    let unknown = |err: String| WebhookSection {
        configured: false,
        last_delivery_at: None,
        last_status: None,
        error: Some(err),
    };
    let pool = state.storage.pool();
    let fut = async {
        // `webhooks` table holds the configured targets; presence of any
        // row = configured. We tolerate the table being absent (older
        // dev binds) by returning `configured: false`.
        let configured: bool =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM webhooks LIMIT 1)")
                .fetch_one(pool)
                .await
                .unwrap_or(false);
        let last: Option<(DateTime<Utc>, i32)> = sqlx::query_as::<_, (DateTime<Utc>, i32)>(
            "SELECT delivered_at, status_code FROM webhook_deliveries \
             ORDER BY delivered_at DESC LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        Ok::<_, sqlx::Error>((configured, last))
    };
    match timeout(DEFAULT_PROBE_TIMEOUT, fut).await {
        Ok(Ok((configured, last))) => WebhookSection {
            configured,
            last_delivery_at: last.map(|(t, _)| t),
            last_status: last.map(|(_, c)| c.to_string()),
            error: None,
        },
        Ok(Err(e)) => unknown(format!("webhook probe failed: {e}")),
        Err(_) => unknown(format!(
            "webhook probe timed out after {}ms",
            DEFAULT_PROBE_TIMEOUT.as_millis()
        )),
    }
}

/// API key inventory. Reads provider names from the configured secret
/// store chain (`opengeo_core::default_chain`). List-only: we report
/// "configured: true/false" per provider; we do NOT return the key
/// value. Bounded by the default timeout because the keyring backend
/// can block on macOS keychain prompts.
pub async fn probe_api_keys() -> Vec<ApiKeyEntry> {
    let providers = opengeo_core::ProviderName::all_wire_names();
    let fut = tokio::task::spawn_blocking(move || {
        use opengeo_core::SecretStore as _;
        let store = opengeo_core::default_chain();
        let mut out = Vec::with_capacity(providers.len());
        for &name in providers {
            // We only check presence — `get` returns NotFound when the
            // key is absent; any other error is reported as "unknown".
            let configured = store.get(name).is_ok();
            out.push(ApiKeyEntry {
                provider: name.to_string(),
                configured,
                last_used_at: None,
            });
        }
        out
    });
    match timeout(DEFAULT_PROBE_TIMEOUT, fut).await {
        Ok(Ok(entries)) => entries,
        // Either timeout or join error — return a "best-effort empty"
        // list. The UI renders "unknown" when all entries are missing.
        Ok(Err(_)) | Err(_) => providers
            .iter()
            .map(|&name| ApiKeyEntry {
                provider: name.to_string(),
                configured: false,
                last_used_at: None,
            })
            .collect(),
    }
}

/// Docker detection. Shells out to `docker version --format
/// '{{.Client.Version}}'` with a 500ms budget. The 500ms is per OQ-P3-22
/// considerations: the install flow that follows is async, so we never
/// want the status probe to block on a slow Docker socket.
pub async fn probe_docker() -> DockerSection {
    let fut = async {
        let output = tokio::process::Command::new("docker")
            .args(["version", "--format", "{{.Client.Version}}"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "docker exited with status {:?}",
                output.status.code()
            ));
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if version.is_empty() {
            return Err("docker returned empty version".to_string());
        }
        Ok::<_, String>(version)
    };
    match timeout(DOCKER_PROBE_TIMEOUT, fut).await {
        Ok(Ok(version)) => DockerSection {
            present: true,
            version: Some(version),
            error: None,
        },
        Ok(Err(e)) => DockerSection {
            present: false,
            version: None,
            error: Some(e),
        },
        Err(_) => DockerSection {
            present: false,
            version: None,
            error: Some(format!(
                "docker probe timed out after {}ms",
                DOCKER_PROBE_TIMEOUT.as_millis()
            )),
        },
    }
}
