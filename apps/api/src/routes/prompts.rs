//! `GET /v1/prompts/run-summary` — Story 0.9 substrate.
//!
//! Aggregates run state per declared prompt within a configurable
//! window (`since`, RFC3339; default = 30 days ago). The shape is
//! consumed by:
//!
//! - MCP tool wrappers needing "is this prompt healthy / how often does
//!   it run / which providers" without listing individual runs.
//! - The Extension's Prompt picker which surfaces avg latency + success
//!   rate before the operator triggers a new run.
//!
//! Rows where the prompt has had zero runs in the window are included
//! with `run_count = 0` and `last_run_at = null` so the Extension can
//! still render the row. Determinism contract: items are ordered by
//! prompt name ascending.
//!
//! `X-OpenGEO-Project` is accepted but not consumed at this layer.

use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use opengeo_core::ProjectId;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/prompts/run-summary", get(run_summary))
}

#[derive(Debug, Deserialize)]
pub struct RunSummaryQuery {
    /// RFC3339 lower bound. Defaults to now()-30d.
    pub since: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptRunSummaryItem {
    pub prompt: String,
    pub run_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<DateTime<Utc>>,
    /// 0.0..=1.0; `None` if `run_count == 0`.
    pub success_rate: Option<f64>,
    /// Mean (finished_at - started_at) in ms over `ok` runs; `None` if
    /// no completed runs.
    pub avg_latency_ms: Option<f64>,
    /// Distinct providers observed in the window, sorted ascending.
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummaryResponse {
    pub items: Vec<PromptRunSummaryItem>,
    /// Echo of the effective lower bound the response was computed
    /// against. Lets clients render "since YYYY-MM-DD" without
    /// re-deriving the default.
    pub since: DateTime<Utc>,
}

async fn run_summary(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<RunSummaryQuery>,
) -> Result<Json<RunSummaryResponse>, (StatusCode, Json<serde_json::Value>)> {
    let since = match q.since.as_deref() {
        None => Utc::now() - Duration::days(30),
        Some(raw) => match DateTime::parse_from_rfc3339(raw) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_since",
                        "message": format!("`since` must be RFC3339: {e}"),
                    })),
                ));
            }
        },
    };

    let items = fetch_summary(&state.storage, project_id, since)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "prompt run-summary fetch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "prompt run-summary fetch failed",
                })),
            )
        })?;

    Ok(Json(RunSummaryResponse { items, since }))
}

async fn fetch_summary(
    storage: &opengeo_storage::Storage,
    project_id: ProjectId,
    since: DateTime<Utc>,
) -> Result<Vec<PromptRunSummaryItem>, sqlx::Error> {
    // LEFT JOIN so prompts with zero runs in window still appear.
    // `array_agg(DISTINCT ...)` gives the per-prompt provider set.
    let rows = sqlx::query(
        r#"
        SELECT
            p.name                                                       AS prompt,
            COUNT(pr.id)::bigint                                         AS run_count,
            MAX(pr.started_at)                                           AS last_run_at,
            SUM(CASE WHEN pr.status = 'ok' THEN 1 ELSE 0 END)::bigint    AS ok_count,
            AVG(
              CASE
                WHEN pr.status = 'ok' AND pr.finished_at IS NOT NULL
                THEN EXTRACT(EPOCH FROM (pr.finished_at - pr.started_at)) * 1000.0
                ELSE NULL
              END
            )::double precision                                          AS avg_latency_ms,
            ARRAY(
              SELECT DISTINCT pr2.provider
              FROM prompt_runs pr2
              WHERE pr2.prompt_id = p.id
                AND pr2.started_at >= $2
              ORDER BY pr2.provider
            )                                                            AS providers
        FROM prompts p
        LEFT JOIN prompt_runs pr
          ON pr.prompt_id = p.id
         AND pr.started_at >= $2
        WHERE p.project_id = $1
        GROUP BY p.id, p.name
        ORDER BY p.name ASC
        "#,
    )
    .bind(project_id)
    .bind(since)
    .fetch_all(storage.pool())
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        let prompt: String = r.try_get("prompt")?;
        let run_count: i64 = r.try_get("run_count")?;
        let last_run_at: Option<DateTime<Utc>> = r.try_get("last_run_at")?;
        let ok_count: i64 = r.try_get("ok_count")?;
        let avg_latency_ms: Option<f64> = r.try_get("avg_latency_ms")?;
        let providers: Vec<String> = r.try_get("providers")?;

        let success_rate = if run_count > 0 {
            Some(ok_count as f64 / run_count as f64)
        } else {
            None
        };

        items.push(PromptRunSummaryItem {
            prompt,
            run_count,
            last_run_at,
            success_rate,
            avg_latency_ms,
            providers,
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_serializes_with_required_fields() {
        let item = PromptRunSummaryItem {
            prompt: "vector-db".into(),
            run_count: 14,
            last_run_at: Some(Utc::now()),
            success_rate: Some(0.93),
            avg_latency_ms: Some(1240.0),
            providers: vec!["anthropic".into(), "openai".into()],
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["prompt"], "vector-db");
        assert_eq!(v["run_count"], 14);
        assert_eq!(v["success_rate"], serde_json::json!(0.93));
        assert_eq!(v["providers"][1], "openai");
    }

    #[test]
    fn item_with_no_runs_serializes_nulls() {
        let item = PromptRunSummaryItem {
            prompt: "dormant".into(),
            run_count: 0,
            last_run_at: None,
            success_rate: None,
            avg_latency_ms: None,
            providers: vec![],
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["run_count"], 0);
        assert!(v["success_rate"].is_null());
        assert!(v["avg_latency_ms"].is_null());
        // last_run_at uses skip_serializing_if so it may be omitted.
        assert!(v.get("last_run_at").map_or(true, |x| x.is_null()));
    }

    #[test]
    fn since_query_parses_rfc3339() {
        // Sanity check that DateTime::parse_from_rfc3339 accepts the
        // documented shape — guards against accidental tightening.
        let parsed = DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z").unwrap();
        assert_eq!(parsed.with_timezone(&Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                   "2026-05-29T12:00:00Z");
    }
}
