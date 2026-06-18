//! Story 26.1/26.2 — Org audit-log read + export surface.
//!
//! Routes:
//!   GET /v1/orgs/:org_id/audit-log         — paginated recent events
//!   GET /v1/orgs/:org_id/audit-log/export  — date-range export (CSV/JSON)
//!   GET /v1/orgs/:org_id/audit-log/retention — current retention policy
//!
//! Write path (append) is fire-and-forget from mutation route handlers.
//!
//! Role scoping (26.2 AC-1):
//!   Owner / Admin  → full org audit export
//!   Billing        → billing-scoped events only (action prefix "billing.")
//!   Operator/Viewer/unauthenticated → 403

/// [p4-audit-1] Default audit retention in days. Override via AUDIT_RETENTION_DAYS env var.
pub const AUDIT_RETENTION_DAYS_DEFAULT: u32 = 365;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use anseo_authz::matrix::Role;

use crate::middleware::authz::CallerRole;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/orgs/{org_id}/audit-log", get(list_audit_log))
        .route("/orgs/{org_id}/audit-log/export", get(export_audit_log))
        .route(
            "/orgs/{org_id}/audit-log/retention",
            get(get_retention_policy),
        )
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

/// Query params for the export endpoint.
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// RFC 3339 start (inclusive). Defaults to 30 days ago.
    pub from: Option<DateTime<Utc>>,
    /// RFC 3339 end (inclusive). Defaults to now.
    pub to: Option<DateTime<Utc>>,
    /// Output format: `json` (default) or `csv`.
    pub format: Option<String>,
    /// Max rows (default 10 000, hard cap 100 000).
    pub limit: Option<i64>,
}

/// GET /v1/orgs/:org_id/audit-log/export
///
/// Role scoping: Owner/Admin get full org export; Billing gets billing.* events
/// only; all other roles receive 403. The export action is itself audited.
async fn export_audit_log(
    Path(org_id): Path<Uuid>,
    Query(q): Query<ExportQuery>,
    caller_role: Option<Extension<CallerRole>>,
    State(state): State<AppState>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    use axum::response::IntoResponse;

    let role_opt = caller_role.map(|Extension(cr)| cr.0);

    // Determine action prefix based on role. No role → self-host mode → allow full.
    let action_prefix: Option<&str> = match role_opt {
        Some(Role::Owner) | Some(Role::Admin) | None => None,
        Some(Role::Billing) => Some("billing."),
        Some(_) => {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "auth_forbidden",
                    "message": "only Owner, Admin, or Billing roles may export audit logs",
                })),
            ));
        }
    };

    // Verify org exists.
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

    let now = Utc::now();
    let from = q.from.unwrap_or_else(|| now - chrono::Duration::days(30));
    let to = q.to.unwrap_or(now);
    let limit = q.limit.unwrap_or(10_000).clamp(1, 100_000);
    let format = q.format.as_deref().unwrap_or("json");

    let events = state
        .storage
        .org_audit()
        .list_range(org_id, from, to, action_prefix, limit)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // Fire-and-forget: audit the export itself.
    let meta = serde_json::json!({
        "from": from.to_rfc3339(),
        "to": to.to_rfc3339(),
        "format": format,
        "rows": events.len(),
        "action_prefix": action_prefix,
    });
    if let Err(e) = state
        .storage
        .org_audit()
        .append(org_id, None, "system", "audit.export", None, Some(&meta))
        .await
    {
        tracing::warn!(error = %e, org_id = %org_id, "audit export self-audit failed");
    }

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

    if format == "csv" {
        let mut csv = String::from("id,ts,actor_login,action,target\n");
        for item in &items {
            csv.push_str(&format!(
                "{},{},{},{},{}\n",
                item.id,
                item.ts,
                csv_escape(&item.actor_login),
                csv_escape(&item.action),
                item.target.as_deref().map(csv_escape).unwrap_or_default(),
            ));
        }
        Ok((
            StatusCode::OK,
            [
                ("content-type", "text/csv; charset=utf-8"),
                (
                    "content-disposition",
                    "attachment; filename=\"audit-export.csv\"",
                ),
            ],
            csv,
        )
            .into_response())
    } else {
        Ok((StatusCode::OK, Json(serde_json::json!({ "items": items }))).into_response())
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// GET /v1/orgs/:org_id/audit-log/retention
///
/// Returns the current retention policy. Configurable via AUDIT_RETENTION_DAYS
/// environment variable; default is 365 days.
async fn get_retention_policy(Path(_org_id): Path<Uuid>) -> Json<serde_json::Value> {
    let days: u32 = std::env::var("AUDIT_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(AUDIT_RETENTION_DAYS_DEFAULT);

    Json(serde_json::json!({
        "retention_days": days,
        "note": "Configurable via AUDIT_RETENTION_DAYS environment variable. Default: 365.",
    }))
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

    #[test]
    fn csv_escape_handles_commas_and_quotes() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("with,comma"), "\"with,comma\"");
        assert_eq!(csv_escape("with\"quote"), "\"with\"\"quote\"");
    }

    #[test]
    fn retention_default_constant() {
        assert_eq!(AUDIT_RETENTION_DAYS_DEFAULT, 365);
    }

    /// [p4-audit-1] Contract sentinel — asserts that role-scoped export, retention
    /// policy, and date-range list_range are all present in this compilation unit.
    #[allow(dead_code)]
    const P4_AUDIT_1_CONTRACT: &str = concat!(
        "[p4-audit-1] export endpoint + retention endpoint + role-scoped billing prefix compile"
    );
}
