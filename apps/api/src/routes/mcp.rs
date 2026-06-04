#![allow(clippy::doc_overindented_list_items)]
//! Story 16.8 — `/v1/mcp/*` backend endpoints.
//!
//! Three endpoints:
//!
//! - `GET  /v1/mcp/tools`           — returns the static 6-tool registry as JSON.
//! - `GET  /v1/mcp/calls`           — returns recent `mcp_tool_calls` rows
//!                                     (query: `limit=<n>`, `before=<uuid>`).
//! - `GET  /v1/mcp/stats?tool=<n>`  — aggregate stats for one tool (Story 16.9).
//!
//! Auth: routes mount inside the standard `/v1` auth gate.
//! SQLX: uses runtime string queries (not `query!` macros) for SQLX_OFFLINE
//! compatibility.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

// ─────────────────────────────────────────────────────────────────────────────
// Router
// ─────────────────────────────────────────────────────────────────────────────

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/mcp/tools", get(get_mcp_tools))
        .route("/mcp/calls", get(get_mcp_calls))
        .route("/mcp/stats", get(get_mcp_stats))
}

// ─────────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct McpToolInfo {
    pub id: &'static str,
    pub sig: &'static str,
    pub doc: &'static str,
    /// Categorization for the tool browser sidebar.
    /// One of: "visibility" | "runs" | "analytics" | "search"
    pub category: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolsResponse {
    pub tools: Vec<McpToolInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpCallRow {
    pub id: Uuid,
    pub tool_name: String,
    pub status: String,
    pub latency_ms: i32,
    pub error_kind: Option<String>,
    pub called_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpCallsResponse {
    pub calls: Vec<McpCallRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpToolStats {
    pub tool_name: String,
    pub total_calls: i64,
    pub ok_calls: i64,
    pub error_calls: i64,
    pub error_rate: f64,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Static tool registry
// ─────────────────────────────────────────────────────────────────────────────

const TOOLS: &[McpToolInfo] = &[
    McpToolInfo {
        id: "run_prompt",
        sig: "(prompt: string, providers?: string[])",
        doc: "Execute a prompt across selected providers.",
        category: "runs",
    },
    McpToolInfo {
        id: "get_visibility",
        sig: "(prompt: string, days: number = 7)",
        doc: "Return visibility trend points for a prompt.",
        category: "visibility",
    },
    McpToolInfo {
        id: "compare_brands",
        sig: "(brands: string[], prompt: string)",
        doc: "Side-by-side ranking comparison.",
        category: "visibility",
    },
    McpToolInfo {
        id: "get_citations",
        sig: "(prompt?: string, since?: string)",
        doc: "Aggregated citations with frequency + source type.",
        category: "analytics",
    },
    McpToolInfo {
        id: "list_trends",
        sig: "(days: number = 30)",
        doc: "Volatility + delta leaderboard across all prompts.",
        category: "analytics",
    },
    McpToolInfo {
        id: "search_benchmarks",
        sig: "(query: string)",
        doc: "Public benchmark dataset search.",
        category: "search",
    },
    McpToolInfo {
        id: "audit",
        sig: "(target: string, max_pages?: number, fail_on?: string[])",
        doc: "Crawl a URL/sitemap and score citation-readiness (Identity, Extractability, Corroboration).",
        category: "analytics",
    },
];

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/mcp/tools
// ─────────────────────────────────────────────────────────────────────────────

async fn get_mcp_tools() -> Json<McpToolsResponse> {
    Json(McpToolsResponse {
        tools: TOOLS.to_vec(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/mcp/calls
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CallsParams {
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub before: Option<Uuid>,
}

fn default_limit() -> i64 {
    20
}

#[allow(clippy::result_large_err)]
async fn get_mcp_calls(
    State(state): State<AppState>,
    Query(params): Query<CallsParams>,
) -> Result<Json<McpCallsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.storage.pool();
    let limit = params.limit.clamp(1, 200);

    let rows = sqlx::query(
        "SELECT id, tool_name, status, latency_ms, error_kind, called_at \
         FROM mcp_tool_calls ORDER BY called_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let calls = rows
        .into_iter()
        .map(|row| McpCallRow {
            id: row.get::<Uuid, _>("id"),
            tool_name: row.get::<String, _>("tool_name"),
            status: row.get::<String, _>("status"),
            latency_ms: row.get::<i32, _>("latency_ms"),
            error_kind: row.get::<Option<String>, _>("error_kind"),
            called_at: row.get::<DateTime<Utc>, _>("called_at"),
        })
        .collect();

    Ok(Json(McpCallsResponse { calls }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/mcp/stats?tool=<name>
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StatsParams {
    pub tool: String,
}

#[allow(clippy::result_large_err)]
async fn get_mcp_stats(
    State(state): State<AppState>,
    Query(params): Query<StatsParams>,
) -> Result<Json<McpToolStats>, (StatusCode, Json<serde_json::Value>)> {
    let pool = state.storage.pool();
    let tool_name = params.tool;

    let row = sqlx::query(
        "SELECT COUNT(*) AS total_calls, \
         COUNT(*) FILTER (WHERE status = 'ok') AS ok_calls, \
         COUNT(*) FILTER (WHERE status = 'error') AS error_calls, \
         PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY latency_ms) AS p50_ms, \
         PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY latency_ms) AS p95_ms \
         FROM mcp_tool_calls WHERE tool_name = $1",
    )
    .bind(&tool_name)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let total_calls: i64 = row.get::<i64, _>("total_calls");
    let ok_calls: i64 = row.get::<i64, _>("ok_calls");
    let error_calls: i64 = row.get::<i64, _>("error_calls");
    let p50_ms: Option<f64> = row.get::<Option<f64>, _>("p50_ms");
    let p95_ms: Option<f64> = row.get::<Option<f64>, _>("p95_ms");
    let error_rate = error_calls as f64 / total_calls.max(1) as f64;

    Ok(Json(McpToolStats {
        tool_name,
        total_calls,
        ok_calls,
        error_calls,
        error_rate,
        p50_ms,
        p95_ms,
    }))
}
