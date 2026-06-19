//! Story 27.11 — Operator profile / security: MFA lifecycle + sessions.
//!
//! authz: check_authz — write endpoints are guarded by `operator_id_from_ctx`,
//! which enforces authenticated operator identity (session-scoped, no RBAC
//! capability required — operators may only act on their own sessions/MFA).
//!
//! GET    /v1/account/sessions          — list active sessions for the operator
//! DELETE /v1/account/sessions/:id      — revoke a session
//! GET    /v1/account/mfa              — MFA enrollment status
//! DELETE /v1/account/mfa             — revoke active TOTP enrollment
//!
//! MFA enrollment flow (re-enroll / new phone):
//!   POST /auth/mfa/enroll  (existing route, Story 21.3) → TOTP secret + QR
//!   POST /auth/mfa/challenge → confirm + mark confirmed_at
//!
//! Recovery codes are generated at enrollment and returned once (plaintext),
//! then stored as SHA-256 hashes. This route surface covers the management
//! plane only; the auth flow lives in auth routes.

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::middleware::org_guc::OrgContext;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/account/sessions", get(list_sessions))
        .route("/account/sessions/:session_id", delete(revoke_session))
        .route(
            "/account/mfa",
            get(mfa_status).delete(revoke_mfa_enrollment),
        )
}

fn operator_id_from_ctx(
    org_context: Option<Extension<OrgContext>>,
) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)> {
    org_context
        .and_then(|Extension(ctx)| ctx.operator_id)
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "operator identity required"})),
            )
        })
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: Uuid,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

async fn list_sessions(
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let operator_id = operator_id_from_ctx(org_context)?;

    type SessionRow = (
        Uuid,
        Option<String>,
        Option<String>,
        DateTime<Utc>,
        DateTime<Utc>,
        DateTime<Utc>,
    );

    let rows: Vec<SessionRow> = sqlx::query_as(
        r#"
        SELECT id, user_agent, ip_address::text,
               last_active_at, expires_at, created_at
        FROM operator_sessions
        WHERE operator_id = $1
          AND revoked_at IS NULL
          AND expires_at > now()
        ORDER BY last_active_at DESC
        LIMIT 50
        "#,
    )
    .bind(operator_id)
    .fetch_all(state.storage.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("storage: {e}")})),
        )
    })?;

    let sessions: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(id, ua, ip, last, exp, created)| {
            serde_json::json!({
                "id": id,
                "user_agent": ua,
                "ip_address": ip,
                "last_active_at": last,
                "expires_at": exp,
                "created_at": created,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

async fn revoke_session(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let operator_id = operator_id_from_ctx(org_context)?;

    let result = sqlx::query(
        r#"
        UPDATE operator_sessions
        SET revoked_at = now()
        WHERE id = $1 AND operator_id = $2 AND revoked_at IS NULL
        "#,
    )
    .bind(session_id)
    .bind(operator_id)
    .execute(state.storage.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("storage: {e}")})),
        )
    })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "session not found or already revoked"})),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// MFA status + revocation
// ---------------------------------------------------------------------------

async fn mfa_status(
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let operator_id = operator_id_from_ctx(org_context)?;

    let row: Option<(Uuid, Option<DateTime<Utc>>, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT id, confirmed_at, created_at
        FROM totp_enrollments
        WHERE operator_id = $1
          AND confirmed_at IS NOT NULL
          AND revoked_at IS NULL
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(operator_id)
    .fetch_optional(state.storage.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("storage: {e}")})),
        )
    })?;

    match row {
        Some((id, confirmed_at, created_at)) => Ok(Json(serde_json::json!({
            "enrolled": true,
            "enrollment_id": id,
            "confirmed_at": confirmed_at,
            "created_at": created_at,
        }))),
        None => Ok(Json(serde_json::json!({ "enrolled": false }))),
    }
}

async fn revoke_mfa_enrollment(
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let operator_id = operator_id_from_ctx(org_context)?;

    let result = sqlx::query(
        r#"
        UPDATE totp_enrollments
        SET revoked_at = now()
        WHERE operator_id = $1
          AND confirmed_at IS NOT NULL
          AND revoked_at IS NULL
        "#,
    )
    .bind(operator_id)
    .execute(state.storage.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("storage: {e}")})),
        )
    })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "no active MFA enrollment found"})),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
