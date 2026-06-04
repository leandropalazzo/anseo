//! Roadmap Epic 31 — crawler observability API and Grafana datasource shim.

use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use opengeo_crawler_ingest::metrics::{MetricsParams, MetricsStore};
use opengeo_crawler_ingest::{
    AccessLogAdapter, AccessLogFormat, BotRangeVerifier, IngestSink, PostgresCrawlerSink,
    PrivacyMode,
};
use serde::{Deserialize, Serialize};

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/crawlers/metrics", get(metrics_handler))
        .route("/crawlers/ratio", get(ratio_handler))
        .route("/crawlers/ingest", post(ingest_handler))
        .route("/grafana/crawlers/query", post(grafana_query_handler))
}

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    pub days: Option<i64>,
    pub include_unverified: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct GrafanaQuery {
    pub days: Option<i64>,
    pub target: Option<String>,
    pub include_unverified: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct GrafanaSeries {
    pub target: String,
    pub datapoints: Vec<(i64, i64)>,
}

async fn metrics_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<opengeo_crawler_ingest::CrawlerMetrics>, (StatusCode, Json<serde_json::Value>)> {
    let days = validate_days(q.days)?;
    let store = MetricsStore::from_storage(&state.storage);
    let metrics = store
        .fetch(MetricsParams {
            project_id,
            days,
            include_unverified: q.include_unverified.unwrap_or(false),
        })
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "crawlers.metrics", "fetch failed");
            err_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "crawler metrics fetch failed",
            )
        })?;
    Ok(Json(metrics))
}

async fn ratio_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<MetricsQuery>,
) -> Result<
    Json<opengeo_crawler_ingest::metrics::CrawlReferReport>,
    (StatusCode, Json<serde_json::Value>),
> {
    let days = validate_days(q.days)?;
    let store = MetricsStore::from_storage(&state.storage);
    let report = store
        .fetch_crawl_refer_ratio(MetricsParams {
            project_id,
            days,
            include_unverified: false,
        })
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "crawlers.ratio", "fetch failed");
            err_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "crawler ratio fetch failed",
            )
        })?;
    Ok(Json(report))
}

async fn grafana_query_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Json(q): Json<GrafanaQuery>,
) -> Result<Json<Vec<GrafanaSeries>>, (StatusCode, Json<serde_json::Value>)> {
    let days = validate_days(q.days)?;
    let store = MetricsStore::from_storage(&state.storage);
    let metrics = store
        .fetch(MetricsParams {
            project_id,
            days,
            include_unverified: q.include_unverified.unwrap_or(false),
        })
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "grafana.crawlers.query", "fetch failed");
            err_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "crawler grafana query failed",
            )
        })?;

    let target = q.target.unwrap_or_else(|| "verified crawler hits".into());
    let datapoints = metrics
        .trend
        .iter()
        .filter_map(|bucket| {
            chrono::NaiveDate::parse_from_str(&bucket.day, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| (bucket.hits, dt.and_utc().timestamp_millis()))
        })
        .collect();
    Ok(Json(vec![GrafanaSeries { target, datapoints }]))
}

#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    /// Raw web-server access-log lines (nginx/Apache). One hit per line.
    pub lines: Vec<String>,
    /// Log format: `common` or `combined` (default `combined`).
    #[serde(default)]
    pub format: Option<String>,
    /// Privacy mode for client IPs: `hashed` (default), `truncated`, or `raw`.
    #[serde(default)]
    pub privacy_mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IngestResult {
    /// Lines that parsed into a recognized crawler hit.
    pub parsed: usize,
    /// Normalized events newly written (idempotent on source+raw_event_id).
    pub ingested: u64,
    /// Lines that did not parse (malformed or non-crawler user-agents).
    pub skipped: usize,
}

fn parse_format(raw: Option<&str>) -> AccessLogFormat {
    match raw.map(str::to_ascii_lowercase).as_deref() {
        Some("common") => AccessLogFormat::Common,
        _ => AccessLogFormat::Combined,
    }
}

fn parse_privacy_mode(raw: Option<&str>) -> PrivacyMode {
    match raw.map(str::to_ascii_lowercase).as_deref() {
        Some("raw") => PrivacyMode::Raw,
        Some("truncated") => PrivacyMode::Truncated,
        _ => PrivacyMode::Hashed,
    }
}

/// `POST /v1/crawlers/ingest` — accept raw access-log lines, normalize them via
/// the in-tree access-log adapter, verify bot identity, and write them to the
/// project's crawler-event sink. The "fire it up from the UI" path: paste a
/// slice of your server logs and they appear in `/v1/crawlers/metrics`.
async fn ingest_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Json(req): Json<IngestRequest>,
) -> Result<Json<IngestResult>, (StatusCode, Json<serde_json::Value>)> {
    if req.lines.is_empty() {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "empty_ingest",
            "`lines` must contain at least one access-log line",
        ));
    }
    let format = parse_format(req.format.as_deref());
    let privacy_mode = parse_privacy_mode(req.privacy_mode.as_deref());
    // Per-deployment salt; only meaningful in hashed mode. Stable across calls
    // so the same IP hashes identically.
    let salt =
        std::env::var("OGEO_CRAWLER_PRIVACY_SALT").unwrap_or_else(|_| "opengeo-crawler".into());
    let verifier = BotRangeVerifier::default();

    let mut events = Vec::new();
    let mut skipped = 0usize;
    for (i, line) in req.lines.iter().enumerate() {
        if line.trim().is_empty() {
            skipped += 1;
            continue;
        }
        match AccessLogAdapter::parse_line(i as u64, line, format, None, "api_ingest") {
            Some(hit) => {
                let ip_verified = hit
                    .client_ip
                    .as_deref()
                    .map(|ip| verifier.verify_user_agent_ip(&hit.user_agent, ip))
                    .unwrap_or(false);
                match hit.normalize(project_id, privacy_mode, &salt, ip_verified) {
                    Some(event) => events.push(event),
                    None => skipped += 1, // non-crawler user-agent
                }
            }
            None => skipped += 1,
        }
    }

    let parsed = events.len();
    let sink = PostgresCrawlerSink::from_storage(&state.storage);
    let ingested = sink.insert_events(&events).await.map_err(|e| {
        tracing::error!(error = %e, route = "crawlers.ingest", "sink insert failed");
        err_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "crawler ingest write failed",
        )
    })?;

    Ok(Json(IngestResult {
        parsed,
        ingested,
        skipped,
    }))
}

fn validate_days(value: Option<i64>) -> Result<i64, (StatusCode, Json<serde_json::Value>)> {
    let v = value.unwrap_or(30);
    if !(1..=365).contains(&v) {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_window",
            "`days` must be in [1, 365]",
        ));
    }
    Ok(v)
}

fn err_body(
    status: StatusCode,
    error: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "error": error,
            "message": message,
        })),
    )
}
