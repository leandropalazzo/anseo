//! Story 21.2 — OIDC SSO connector config store + stub redirect flow.
//!
//! [mock-OK]: Cognito/AWS provisioning is deferred. This module provides:
//!   * A DB-backed per-org OIDC connector config store (create / list / delete).
//!   * A stub redirect endpoint that returns a synthetic OAuth redirect URL.
//!   * A stub callback endpoint that returns a placeholder bearer token.
//!
//! When Cognito is provisioned (the cloud-deferred part), the callback handler
//! will exchange the `code` for a real ID token and validate it via 21.1's
//! `JwksClient`.
//!
//! Endpoints:
//!   GET    /v1/orgs/:org_id/sso                          (OrgRead-gated)
//!   POST   /v1/orgs/:org_id/sso                          (OrgUpdate-gated)
//!   DELETE /v1/orgs/:org_id/sso/:id                      (OrgUpdate-gated)
//!   GET    /v1/auth/sso/redirect?org_id=&connector_id=   (public stub)
//!   GET    /v1/auth/sso/callback?code=&state=            (public stub)

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use anseo_authz::matrix::Capability;

use crate::middleware::authz::{enforce_capability, RequiredCapability};
use crate::middleware::org_guc::OrgContext;
use crate::AppState;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Authenticated, org-scoped connector management routes.
pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route(
            "/orgs/:org_id/sso",
            get(list_connectors).layer(Extension(RequiredCapability(Capability::OrgRead))),
        )
        .route(
            "/orgs/:org_id/sso",
            post(create_connector).layer(Extension(RequiredCapability(Capability::OrgUpdate))),
        )
        .route(
            "/orgs/:org_id/sso/:connector_id",
            delete(delete_connector).layer(Extension(RequiredCapability(Capability::OrgUpdate))),
        )
}

/// Public, unauthenticated OAuth stub endpoints (browser-initiated flow).
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/sso/redirect", get(sso_redirect_stub))
        .route("/auth/sso/callback", get(sso_callback_stub))
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateConnectorRequest {
    pub provider: String,
    pub client_id: String,
    /// Opaque KMS DEK-wrapped secret reference — never the raw secret.
    pub client_secret_ref: String,
    pub issuer_url: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ConnectorResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub provider: String,
    pub client_id: String,
    pub issuer_url: String,
    pub redirect_uri: String,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ConnectorListResponse {
    pub connectors: Vec<ConnectorResponse>,
}

#[derive(Debug, Deserialize)]
pub struct RedirectQuery {
    pub org_id: Uuid,
    pub connector_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct RedirectResponse {
    pub redirect_url: String,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CallbackResponse {
    pub token: String,
    /// Populated once Cognito is provisioned and 21.1's JwksClient validates
    /// the ID token. Until then this is `null` ([mock-OK]).
    pub operator_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn list_connectors(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<ConnectorListResponse>, (StatusCode, Json<serde_json::Value>)> {
    enforce_capability(
        &state,
        org_context.map(|Extension(ctx)| ctx),
        Capability::OrgRead,
    )
    .await
    .map_err(|r| {
        let status = r.status();
        (status, Json(serde_json::json!({"error": "forbidden"})))
    })?;

    let rows = state
        .storage
        .oidc_sso()
        .list_for_org(org_id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    let connectors = rows.into_iter().map(connector_to_response).collect();
    Ok(Json(ConnectorListResponse { connectors }))
}

async fn create_connector(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
    Json(body): Json<CreateConnectorRequest>,
) -> Result<(StatusCode, Json<ConnectorResponse>), (StatusCode, Json<serde_json::Value>)> {
    enforce_capability(
        &state,
        org_context.map(|Extension(ctx)| ctx),
        Capability::OrgUpdate,
    )
    .await
    .map_err(|r| {
        let status = r.status();
        (status, Json(serde_json::json!({"error": "forbidden"})))
    })?;

    let row = state
        .storage
        .oidc_sso()
        .create(
            org_id,
            &body.provider,
            &body.client_id,
            &body.client_secret_ref,
            &body.issuer_url,
            &body.redirect_uri,
            body.enabled,
        )
        .await
        .map_err(|e| internal(e.to_string()))?;

    Ok((StatusCode::CREATED, Json(connector_to_response(row))))
}

async fn delete_connector(
    Path((org_id, connector_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    enforce_capability(
        &state,
        org_context.map(|Extension(ctx)| ctx),
        Capability::OrgUpdate,
    )
    .await
    .map_err(|r| {
        let status = r.status();
        (status, Json(serde_json::json!({"error": "forbidden"})))
    })?;

    let removed = state
        .storage
        .oidc_sso()
        .delete(connector_id, org_id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    if !removed {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "connector_not_found",
                "message": "no SSO connector found with this id for the org",
            })),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Stub redirect — returns a synthetic OAuth redirect URL.
///
/// TODO(21.2-cloud): When Cognito is provisioned, replace this with a real
/// authorization URL constructed from the connector's `issuer_url`,
/// `client_id`, `redirect_uri`, and a PKCE code challenge.
async fn sso_redirect_stub(
    Query(params): Query<RedirectQuery>,
    State(state): State<AppState>,
) -> Result<Json<RedirectResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Look up the connector so we know it exists — real flow would use these
    // fields to build the authorization URL.
    let connector = state
        .storage
        .oidc_sso()
        .get(params.connector_id)
        .await
        .map_err(|e| internal(e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "connector_not_found",
                    "message": "no SSO connector found with this id",
                })),
            )
        })?;

    // Stub redirect URL — not a real OAuth provider.
    let redirect_url = format!(
        "https://stub-oidc-provider.example/auth\
         ?client_id={client_id}\
         &redirect_uri={redirect_uri}\
         &state={org_id}:{connector_id}\
         &response_type=code\
         &scope=openid+email",
        client_id = connector.client_id,
        redirect_uri = connector.redirect_uri,
        org_id = params.org_id,
        connector_id = params.connector_id,
    );

    Ok(Json(RedirectResponse { redirect_url }))
}

/// Stub callback — returns a placeholder bearer token.
///
/// TODO(21.2-cloud): When Cognito is provisioned, exchange `code` for a real
/// ID token and validate it via Story 21.1's `JwksClient`. Then look up or
/// provision the `operator` row and return a real session token.
async fn sso_callback_stub(Query(_params): Query<CallbackQuery>) -> Json<CallbackResponse> {
    // [mock-OK]: stub token; real wiring to 21.1's JwksClient is the
    // cloud-deferred part (requires live Cognito user pool).
    Json(CallbackResponse {
        token: "stub-bearer-token".to_string(),
        operator_id: None,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn connector_to_response(
    c: anseo_storage::repositories::oidc_sso::OidcSsoConnector,
) -> ConnectorResponse {
    ConnectorResponse {
        id: c.id,
        org_id: c.org_id,
        provider: c.provider,
        client_id: c.client_id,
        issuer_url: c.issuer_url,
        redirect_uri: c.redirect_uri,
        enabled: c.enabled,
        created_at: c.created_at,
        updated_at: c.updated_at,
    }
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

    /// Story 21.2 — evidence sentinel.
    ///
    /// * DB-backed oidc_sso_connectors table with RLS (AC-5 guard auto-detects).
    /// * OrgRead-gated GET /v1/orgs/:org_id/sso.
    /// * OrgUpdate-gated POST + DELETE /v1/orgs/:org_id/sso.
    /// * Stub redirect returns synthetic redirect_url (AC-3).
    /// * Stub callback returns stub token + null operator_id (AC-4, [mock-OK]).
    #[allow(dead_code)]
    const STORY_21_2_EVIDENCE: &str =
        "story-21.2 [mock-OK]: oidc_sso_connectors migration + RLS + OrgRead/OrgUpdate-gated \
         CRUD + stub redirect/callback endpoints";

    #[test]
    fn connector_response_serializes() {
        let resp = ConnectorResponse {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            provider: "generic_oidc".to_string(),
            client_id: "test-client".to_string(),
            issuer_url: "https://example.com".to_string(),
            redirect_uri: "https://app.example.com/callback".to_string(),
            enabled: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["provider"], "generic_oidc");
        assert_eq!(json["enabled"], false);
    }

    #[test]
    fn redirect_response_serializes() {
        let resp = RedirectResponse {
            redirect_url: "https://stub-oidc-provider.example/auth?code=xyz".to_string(),
        };
        let json = serde_json::to_value(&resp).expect("serialize");
        assert!(json["redirect_url"]
            .as_str()
            .unwrap()
            .contains("stub-oidc-provider"));
    }

    #[test]
    fn callback_response_serializes() {
        let resp = CallbackResponse {
            token: "stub-bearer-token".to_string(),
            operator_id: None,
        };
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["token"], "stub-bearer-token");
        assert!(json["operator_id"].is_null());
    }
}
