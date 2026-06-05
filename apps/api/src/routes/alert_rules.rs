//! Story 31-1 — `/v1/alert-rules` CRUD surface backing the Ops console
//! Alerts tab. Mirrors the wire shape of `apps/web/lib/mock-ops.ts`
//! `AlertRule`: `{ name, on, target, channels, status, fires }`.
//!
//! Error pattern follows `routes::anomalies` / `routes::schedules`:
//! handlers return `(StatusCode, Json<serde_json::Value>)` on failure
//! rather than a crate-level error enum (none exists in this crate).
//!
//! The repo is constructed inline from `state.storage.pool()`, matching the
//! `routes::prompt_runs` convention (AppState carries no per-table repo
//! handles).
//!
//! Auth: mounted inside the `/v1` surface in `apps/api/src/lib.rs`, so the
//! `require_api_key` + project-header guards apply uniformly with the other
//! `/v1` routes; route paths here are relative to that `/v1` nest.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;
use anseo_storage::repositories::alert_rules::{AlertRuleRecord, AlertRulesRepo};

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route(
            "/alert-rules",
            get(list_alert_rules).post(create_alert_rule),
        )
        .route(
            "/alert-rules/:name",
            axum::routing::patch(patch_alert_rule).delete(delete_alert_rule),
        )
}

type ApiResult<T> = Result<T, (StatusCode, Json<serde_json::Value>)>;

fn err(status: StatusCode, error: &str, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

// ---------------------------------------------------------------------------
// Wire shapes — mirror `apps/web/lib/mock-ops.ts` AlertRule
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AlertRuleResponse {
    pub name: String,
    /// UI field `on` (condition expression). Stored in DB as `condition`.
    pub on: String,
    pub target: String,
    pub channels: Vec<String>,
    pub status: String,
    /// last-7d fire count.
    pub fires: i64,
}

impl AlertRuleResponse {
    fn from_record(r: AlertRuleRecord, fires: i64) -> Self {
        Self {
            name: r.name,
            on: r.condition,
            target: r.target,
            channels: r.channels.0,
            status: r.status,
            fires,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListAlertRulesResponse {
    pub items: Vec<AlertRuleResponse>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAlertRuleBody {
    pub name: String,
    pub on: String,
    pub target: String,
    #[serde(default)]
    pub channels: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchAlertRuleBody {
    pub status: String,
}

// TODO(31-1): attribute fires per rule. `webhook_deliveries` (the source
// `routes::anomalies` reads) carries no rule_id/rule_name column, so
// deliveries cannot be cleanly attributed to a specific alert rule yet.
// Return 0 rather than inventing a join that does not hold.
fn fires_for_rule(_name: &str) -> i64 {
    0
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_alert_rules(
    State(state): State<AppState>,
) -> ApiResult<Json<ListAlertRulesResponse>> {
    let repo = AlertRulesRepo::new(state.storage.pool());
    let rows = repo.list().await.map_err(|e| {
        tracing::error!(error = %e, route = "alert_rules", "list failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "alert-rules list failed",
        )
    })?;
    let items = rows
        .into_iter()
        .map(|r| {
            let fires = fires_for_rule(&r.name);
            AlertRuleResponse::from_record(r, fires)
        })
        .collect();
    Ok(Json(ListAlertRulesResponse { items }))
}

async fn create_alert_rule(
    State(state): State<AppState>,
    Json(body): Json<CreateAlertRuleBody>,
) -> ApiResult<Json<AlertRuleResponse>> {
    if body.name.trim().is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "`name` is required",
        ));
    }
    let repo = AlertRulesRepo::new(state.storage.pool());
    let r = repo
        .create(&body.name, &body.on, &body.target, &body.channels)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "alert_rules", "create failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "alert-rule create failed",
            )
        })?;
    let fires = fires_for_rule(&r.name);
    Ok(Json(AlertRuleResponse::from_record(r, fires)))
}

async fn patch_alert_rule(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<PatchAlertRuleBody>,
) -> ApiResult<Json<AlertRuleResponse>> {
    if body.status != "armed" && body.status != "muted" {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "`status` must be 'armed' or 'muted'",
        ));
    }
    let repo = AlertRulesRepo::new(state.storage.pool());
    let updated = repo.set_status(&name, &body.status).await.map_err(|e| {
        tracing::error!(error = %e, route = "alert_rules", "patch failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "alert-rule update failed",
        )
    })?;
    let r = updated.ok_or_else(|| {
        err(
            StatusCode::NOT_FOUND,
            "not_found",
            "no alert rule with that name",
        )
    })?;
    let fires = fires_for_rule(&r.name);
    Ok(Json(AlertRuleResponse::from_record(r, fires)))
}

async fn delete_alert_rule(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = AlertRulesRepo::new(state.storage.pool());
    let affected = repo.delete(&name).await.map_err(|e| {
        tracing::error!(error = %e, route = "alert_rules", "delete failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "alert-rule delete failed",
        )
    })?;
    if affected == 0 {
        return Err(err(
            StatusCode::NOT_FOUND,
            "not_found",
            "no alert rule with that name",
        ));
    }
    Ok(Json(serde_json::json!({ "deleted": name })))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use anseo_storage::repositories::alert_rules::AlertRulesRepo;
    use sqlx::PgPool;
    use uuid::Uuid;

    #[sqlx::test(migrations = "../../crates/storage/migrations")]
    async fn create_list_patch_delete_round_trip(pool: PgPool) {
        // A default project is required for the project-scoped insert.
        sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
            .bind(Uuid::new_v4())
            .bind("default")
            .execute(&pool)
            .await
            .unwrap();

        let repo = AlertRulesRepo::new(&pool);

        // create
        let created = repo
            .create(
                "latency-spike",
                "p95_latency_ms > 2000",
                "*",
                &["slack:#ops".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(created.name, "latency-spike");
        assert_eq!(created.condition, "p95_latency_ms > 2000");
        assert_eq!(created.target, "*");
        assert_eq!(created.channels.0, vec!["slack:#ops".to_string()]);
        assert_eq!(created.status, "armed");

        // list
        let listed = repo.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "latency-spike");

        // patch -> mute
        let muted = repo
            .set_status("latency-spike", "muted")
            .await
            .unwrap()
            .expect("rule should exist");
        assert_eq!(muted.status, "muted");

        // delete
        let affected = repo.delete("latency-spike").await.unwrap();
        assert_eq!(affected, 1);
        let after = repo.list().await.unwrap();
        assert!(after.is_empty());
    }
}
