//! Story 26.1 — Org audit-log read surface.
//!
//! `GET /v1/orgs/:org_id/audit-log` — list the 200 most recent events for an
//! org, optionally filtered by `action` and `actor`. Requires the caller to be
//! a member of the org (OrgRead capability, enforced by the auth middleware).
//!
//! Write path (append) is fire-and-forget from mutation route handlers — this
//! module only owns the read surface.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/orgs/{org_id}/audit-log", get(list_audit_log))
}

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Filter by exact action key (e.g. "org.create", "org.role.grant").
    pub action: Option<String>,
    /// Filter by actor login.
    pub actor: Option<String>,
    /// Max results (default 200, capped at 1000).
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AuditEventItem {
    pub id: i64,
    pub ts: String,
    pub actor_login: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

async fn list_audit_log(
    Path(org_id): Path<Uuid>,
    Query(q): Query<AuditLogQuery>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Verify the org exists.
    state
        .storage
        .orgs()
        .get(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "org_not_found",
                    "message": "organization not found",
                })),
            )
        })?;

    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let events = state
        .storage
        .org_audit()
        .list(org_id, limit, q.action.as_deref(), q.actor.as_deref())
        .await
        .map_err(|e| internal(e.to_string()))?;

    let items: Vec<AuditEventItem> = events
        .into_iter()
        .map(|e| AuditEventItem {
            id: e.id,
            ts: e.ts.to_rfc3339(),
            actor_login: e.actor_login,
            action: e.action,
            target: e.target,
            metadata: e.metadata,
        })
        .collect();

    Ok(Json(serde_json::json!({ "items": items })))
}

fn internal(msg: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "error": "internal_error",
            "message": msg,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_item_skips_none_fields() {
        let item = AuditEventItem {
            id: 1,
            ts: "2026-06-18T00:00:00Z".into(),
            actor_login: "alice".into(),
            action: "org.create".into(),
            target: None,
            metadata: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("target"));
        assert!(!json.contains("metadata"));
        assert!(json.contains("org.create"));
    }
}
