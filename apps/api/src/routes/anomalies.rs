#![allow(clippy::doc_overindented_list_items)]
//! Phase 3 Story 0.7 — `/v1/anomalies` substrate endpoint.
//!
//! Surfaces the union of scheduler-emitted anomaly events (FR-26a) for the
//! authenticated project, in a wire-stable shape that consumers in epic-16
//! (`list_trends` MCP tool) and epic-19 (Kind 2.10 recommendations) depend
//! on. The substrate is intentionally narrow: it locks the response shape
//! and the query-parameter taxonomy so downstream stories can ship without
//! re-negotiating wire surface.
//!
//! ## Data source
//!
//! Anomalies are emitted by the scheduler as ARCH-17 `LifecycleEvent`s
//! (`visibility.anomaly`, `citation.anomaly`) and persisted at
//! webhook-fanout time onto `webhook_deliveries` (`event_kind`,
//! `payload_jsonb`). This endpoint reads the deduplicated `event_id` set
//! for the authenticated project from that table — one row per anomaly,
//! regardless of how many webhooks fanned it out, regardless of delivery
//! status (the anomaly *happened* even if no webhook subscribed yet).
//!
//! Phase 3 Story 0.7 deliberately does NOT introduce a dedicated
//! `anomalies` table. The webhook-deliveries view is the canonical
//! source-of-truth for emitted anomalies for as long as the scheduler is
//! the only producer; if a future story introduces ephemeral anomalies
//! that bypass the webhook channel, the read path here grows a UNION with
//! that new table.
//!
//! ## Filters
//!
//! - `prompt` / `provider` — optional equality filters. Applied in-memory
//!   over the `detail` payload because the source-of-truth shape varies
//!   per detector kind.
//! - `window` — `1d` | `7d` | `30d`, default `7d`. Clamps `created_at`.
//! - `kind` — `visibility` | `citation` | `all`, default `all`.
//! - `since` — RFC 3339 floor on `created_at`; AND-ed with `window`.

use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

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

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/anomalies", get(list_anomalies_handler))
}

// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

/// Wire-stable taxonomy. `visibility_drop` and `citation_loss` map onto the
/// two detectors in `crates/analytics/src/anomaly`. `rank_swap` is reserved
/// for a future detector — kept in the enum so downstream consumers
/// (`list_trends` MCP tool) can pattern-match without growing a "unknown
/// kind" branch when it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyItemKind {
    VisibilityDrop,
    CitationLoss,
    RankSwap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyItem {
    /// ULID-form ID stable per emission (sourced from the event_id UUID).
    pub id: String,
    pub kind: AnomalyItemKind,
    /// Prompt slug (best-effort extracted from the detector `detail` blob;
    /// `None` if the detector did not record one, e.g. project-wide
    /// citation novelty).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Wire-stable provider name (`openai|anthropic|gemini|perplexity|…`).
    pub provider: String,
    pub detected_at: DateTime<Utc>,
    pub severity: AnomalySeverity,
    /// Effect-size signal: z-score magnitude for visibility, normalized
    /// novelty count for citations. Bounded loosely; consumers MUST treat
    /// as opaque for ranking purposes.
    pub delta: f64,
    pub window_days: u32,
    /// Verbatim detector detail blob.
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomaliesResponse {
    pub items: Vec<AnomalyItem>,
    pub trace_id: String,
}

// ---------------------------------------------------------------------------
// Query parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
enum WindowParam {
    #[serde(rename = "1d")]
    OneDay,
    #[serde(rename = "7d")]
    #[default]
    SevenDay,
    #[serde(rename = "30d")]
    ThirtyDay,
}

impl WindowParam {
    fn days(self) -> u32 {
        match self {
            Self::OneDay => 1,
            Self::SevenDay => 7,
            Self::ThirtyDay => 30,
        }
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
enum KindParam {
    Visibility,
    Citation,
    #[default]
    All,
}


#[derive(Debug, Deserialize)]
pub struct AnomaliesQuery {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    window: Option<WindowParam>,
    #[serde(default)]
    kind: Option<KindParam>,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

async fn list_anomalies_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<AnomaliesQuery>,
) -> Result<Json<AnomaliesResponse>, (StatusCode, Json<serde_json::Value>)> {
    let window = q.window.unwrap_or_default();
    let kind = q.kind.unwrap_or_default();
    let window_days = window.days();

    // SQL `interval` literal; days() is bounded to {1,7,30} by the enum so
    // this is injection-safe.
    let interval = format!("{window_days} days");

    // Kind filter expressed as a fixed array bound to a TEXT[] parameter
    // so the SQL stays a single statement irrespective of which subset is
    // requested.
    let kinds: Vec<String> = match kind {
        KindParam::Visibility => vec!["visibility.anomaly".into()],
        KindParam::Citation => vec!["citation.anomaly".into()],
        KindParam::All => vec!["visibility.anomaly".into(), "citation.anomaly".into()],
    };

    // Hand-rolled `query()` rather than `query_as!` because the JSONB
    // column needs `try_get::<serde_json::Value>` decoding and the
    // workspace sqlx feature set does not enable the macro-side JSONB
    // recognition. Mirrors the pattern in
    // `crates/storage/src/repositories/webhook_deliveries.rs`.
    let raw_rows = sqlx::query(
        r#"
        SELECT DISTINCT ON (d.event_id)
            d.event_id, d.event_kind, d.created_at, d.payload_jsonb
        FROM webhook_deliveries d
        JOIN webhooks w ON w.id = d.webhook_id
        WHERE w.project_id = $1
          AND d.event_kind = ANY($2::text[])
          AND d.created_at >= now() - ($3::text)::interval
          AND ($4::timestamptz IS NULL OR d.created_at >= $4)
        ORDER BY d.event_id, d.created_at DESC
        "#,
    )
    .bind(project_id)
    .bind(kinds)
    .bind(interval)
    .bind(q.since)
    .fetch_all(state.storage.pool())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, route = "anomalies", "fetch failed");
        err_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "anomalies fetch failed",
        )
    })?;

    use sqlx::Row as _;
    let rows: Vec<(Uuid, String, DateTime<Utc>, serde_json::Value)> = raw_rows
        .into_iter()
        .map(|r| {
            (
                r.get::<Uuid, _>("event_id"),
                r.get::<String, _>("event_kind"),
                r.get::<DateTime<Utc>, _>("created_at"),
                r.get::<serde_json::Value, _>("payload_jsonb"),
            )
        })
        .collect();

    let mut items: Vec<AnomalyItem> = rows
        .into_iter()
        .filter_map(|(event_id, event_kind, created_at, payload)| {
            row_to_item(event_id, &event_kind, created_at, payload, window_days)
        })
        .filter(|item| {
            q.prompt
                .as_deref()
                .map(|p| item.prompt.as_deref() == Some(p))
                .unwrap_or(true)
                && q.provider
                    .as_deref()
                    .map(|p| item.provider == p)
                    .unwrap_or(true)
        })
        .collect();

    // Sort by detected_at desc — DISTINCT ON broke the time order on the
    // SQL side.
    items.sort_by_key(|a| std::cmp::Reverse(a.detected_at));

    Ok(Json(AnomaliesResponse {
        items,
        trace_id: trace_id(),
    }))
}

// ---------------------------------------------------------------------------
// Payload projection
// ---------------------------------------------------------------------------

/// Projects a persisted `LifecycleEvent` payload onto an `AnomalyItem`.
///
/// Returns `None` if the payload shape is unrecognized (e.g., a future
/// scheduler emits a kind we don't yet map). The caller treats that as a
/// soft-skip so a malformed history row never 500s the endpoint.
fn row_to_item(
    event_id: Uuid,
    event_kind: &str,
    created_at: DateTime<Utc>,
    payload: serde_json::Value,
    window_days: u32,
) -> Option<AnomalyItem> {
    let kind = match event_kind {
        "visibility.anomaly" => AnomalyItemKind::VisibilityDrop,
        "citation.anomaly" => AnomalyItemKind::CitationLoss,
        _ => return None,
    };

    let provider = payload
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let detected_at = payload
        .get("observed_at")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(created_at);
    let detail = payload.get("detail").cloned().unwrap_or(serde_json::json!({}));

    // Best-effort detail unpacking. The detector's `detail` blob is
    // structurally stable per detector kind (see anomaly::zscore and
    // anomaly::citation_novelty) but we treat any field as optional so a
    // detector schema bump never crashes this endpoint.
    let prompt = detail
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let delta = detail
        .get("z")
        .or_else(|| detail.get("delta"))
        .or_else(|| detail.get("novel_count"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let severity = severity_from_delta(delta);

    Some(AnomalyItem {
        id: ulid::Ulid::from_bytes(event_id.into_bytes()).to_string(),
        kind,
        prompt,
        provider,
        detected_at,
        severity,
        delta,
        window_days,
        details: detail,
    })
}

fn severity_from_delta(delta: f64) -> AnomalySeverity {
    let mag = delta.abs();
    if mag >= 4.0 {
        AnomalySeverity::High
    } else if mag >= 2.5 {
        AnomalySeverity::Medium
    } else {
        AnomalySeverity::Low
    }
}

fn trace_id() -> String {
    // Phase 3 substrate: a fresh ULID per response. Story 16.x will swap
    // this for the W3C traceparent propagated from the request.
    ulid::Ulid::new().to_string()
}
