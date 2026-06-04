//! Roadmap Epic 32 — site-audit REST surface.
//!
//! `POST /v1/audit` runs the in-tree citation-readiness engine
//! (`opengeo_audit`) against a URL/sitemap and returns the scored report.
//! This is the parity backend for the `/audit` dashboard (Story 32-5) and the
//! MCP `audit` tool (Story 32-4) — same operation, three surfaces.

use std::time::Duration;

use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use opengeo_audit::{crawl_and_audit, evaluate_gate, AuditOptions, AuditReport, FailOn};
use serde::{Deserialize, Serialize};

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/audit", post(audit))
        .route("/audit/runs", get(list_runs))
}

#[derive(Debug, Deserialize)]
pub struct AuditRequest {
    /// URL, sitemap URL, `file://` URL, or local HTML fixture path.
    pub target: String,
    #[serde(default)]
    pub max_pages: Option<usize>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Optional CI-gate thresholds (`low`/`medium`/`high` or rule ids).
    #[serde(default)]
    pub fail_on: Vec<String>,
}

async fn audit(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Json(req): Json<AuditRequest>,
) -> Result<Json<AuditReport>, (StatusCode, Json<serde_json::Value>)> {
    if req.target.trim().is_empty() {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_target",
            "`target` must not be empty",
        ));
    }
    let options = AuditOptions {
        max_pages: req.max_pages.unwrap_or(25).clamp(1, 200),
        timeout: Duration::from_millis(req.timeout_ms.unwrap_or(10_000).clamp(1_000, 60_000)),
    };

    let mut report = crawl_and_audit(&req.target, options).await.map_err(|e| {
        tracing::error!(error = %e, route = "audit", "crawl_and_audit failed");
        err_body(
            StatusCode::BAD_GATEWAY,
            "audit_failed",
            "audit crawl failed for the requested target",
        )
    })?;

    if !req.fail_on.is_empty() {
        let fail_on: Vec<FailOn> = req.fail_on.iter().map(|s| FailOn::parse(s)).collect();
        let gate = evaluate_gate(&report, &fail_on);
        report = report.with_gate(gate);
    }

    // Persist the run so citation-readiness can be tracked over time. A storage
    // failure must not fail the audit itself — log and return the report.
    let id = uuid::Uuid::from_bytes(ulid::Ulid::new().to_bytes());
    let gate_passed = report.gate.as_ref().map(|g| g.passed);
    match serde_json::to_value(&report) {
        Ok(report_json) => {
            if let Err(e) = state
                .storage
                .audit()
                .insert_run(
                    id,
                    project_id,
                    &report.target,
                    report.overall_score as i16,
                    report.pages.len() as i32,
                    gate_passed,
                    &report_json,
                )
                .await
            {
                tracing::error!(error = %e, route = "audit", "audit run persist failed");
            }
        }
        Err(e) => tracing::error!(error = %e, route = "audit", "audit report serialize failed"),
    }

    Ok(Json(report))
}

#[derive(Debug, Serialize)]
struct AuditRunItem {
    id: String,
    target: String,
    overall_score: i16,
    pages_crawled: i32,
    gate_passed: Option<bool>,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
}

async fn list_runs(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<RunsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let runs = state
        .storage
        .audit()
        .list_runs_for_project(project_id, limit)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "audit.runs", "list failed");
            err_body(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "audit history fetch failed",
            )
        })?;
    let items: Vec<AuditRunItem> = runs
        .into_iter()
        .map(|r| AuditRunItem {
            id: r.id.to_string(),
            target: r.target,
            overall_score: r.overall_score,
            pages_crawled: r.pages_crawled,
            gate_passed: r.gate_passed,
            created_at: r.created_at.to_rfc3339(),
        })
        .collect();
    Ok(Json(serde_json::json!({ "items": items })))
}

fn err_body(
    status: StatusCode,
    error: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}
