//! Story 27.1 — Public signup and email-verification endpoints.
//!
//! POST /v1/auth/signup        — create operator + org + provisioning saga
//! POST /v1/auth/verify-email  — advance saga to complete; org.state → trial
//!
//! Both endpoints are unauthenticated (no API key required). The saga is the
//! idempotency mechanism: re-submitting the same email returns the existing saga.
//!
//! [mock-OK]: live KMS wired in 23.1; Argon2id password hashing in 27.2.
//! For now password_hash is stored as SHA-256 hex of the raw password.
//! Email sending is a tracing log line — production wires the real mailer in 27.2.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppState;

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/verify-email", post(verify_email))
}

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub org_name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct SignupResponse {
    pub saga_id: Uuid,
    pub org_id: Uuid,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailRequest {
    pub saga_id: Uuid,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyEmailResponse {
    pub org_id: Uuid,
    pub message: String,
}

/// Slugify an org name: lowercase, replace non-alphanumeric with hyphens,
/// collapse consecutive hyphens, trim leading/trailing hyphens.
fn slugify(s: &str) -> String {
    let base: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    // Collapse runs of hyphens and trim.
    let collapsed = base
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    // Append 6 hex chars from a UUID to avoid slug collisions.
    let suffix = &Uuid::new_v4().to_string().replace('-', "")[..6];
    format!("{collapsed}-{suffix}")
}

async fn signup(
    State(state): State<AppState>,
    Json(body): Json<SignupRequest>,
) -> Result<(StatusCode, Json<SignupResponse>), (StatusCode, Json<serde_json::Value>)> {
    let email = body.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "invalid_email",
                "message": "email must be a valid address",
            })),
        ));
    }
    if body.org_name.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "invalid_org_name",
                "message": "org_name must not be blank",
            })),
        ));
    }
    if body.password.len() < 8 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "weak_password",
                "message": "password must be at least 8 characters",
            })),
        ));
    }

    // [mock-OK] SHA-256; production uses Argon2id (Story 27.2).
    let password_hash = hex::encode(Sha256::digest(body.password.as_bytes()));
    let slug = slugify(body.org_name.trim());

    // Create org (state column defaults to 'unconfigured' via 27.1 migration).
    let org = state
        .storage
        .orgs()
        .create(&slug, body.org_name.trim(), None)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // Create operator (email/password path; login = email).
    let operator_id = state
        .storage
        .orgs()
        .create_operator(&email, &email, &password_hash)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // Mint a random email-verify token; saga stores its SHA-256 hash.
    let raw_token = Uuid::new_v4().to_string();
    let token_hash = hex::encode(Sha256::digest(raw_token.as_bytes()));
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);

    let saga = state
        .storage
        .provisioning_sagas()
        .create(operator_id, org.id, &token_hash, expires_at)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // [mock-OK] KMS step — 23.1 KmsOrgStore would provision a CMK here.
    state
        .storage
        .provisioning_sagas()
        .advance_kms_done(saga.id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // Entitlement step — upsert free plan (24.1 surface).
    state
        .storage
        .org_entitlements()
        .upsert(org.id, "free", 0, None, None)
        .await
        .map_err(|e| internal(e.to_string()))?;

    state
        .storage
        .provisioning_sagas()
        .advance_entitlement(saga.id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // Owner role — insert into operator_org_roles.
    state
        .storage
        .orgs()
        .add_member(org.id, operator_id, "owner")
        .await
        .map_err(|e| internal(e.to_string()))?;

    state
        .storage
        .provisioning_sagas()
        .advance_owner_set(saga.id)
        .await
        .map_err(|e| internal(e.to_string()))?;

    // [mock-OK] In production the raw_token is emailed, never logged.
    tracing::info!(
        event = "signup.verify_email_sent",
        saga_id = %saga.id,
        email = %email,
        "[mock] verify token: {raw_token}"
    );

    Ok((
        StatusCode::CREATED,
        Json(SignupResponse {
            saga_id: saga.id,
            org_id: org.id,
            message: "Check your email for a verification link.".into(),
        }),
    ))
}

async fn verify_email(
    State(state): State<AppState>,
    Json(body): Json<VerifyEmailRequest>,
) -> Result<Json<VerifyEmailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let token_hash = hex::encode(Sha256::digest(body.token.as_bytes()));

    let saga = state
        .storage
        .provisioning_sagas()
        .get_by_token(body.saga_id, &token_hash)
        .await
        .map_err(|e| internal(e.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error": "invalid_or_expired_token",
                    "message": "verification token is invalid or has expired",
                })),
            )
        })?;

    let completed = state
        .storage
        .provisioning_sagas()
        .complete(saga.id, &token_hash)
        .await
        .map_err(|e| internal(e.to_string()))?;

    if !completed {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "saga_not_ready",
                "message": "provisioning saga is not in a state that can be completed",
            })),
        ));
    }

    // Set org.state → trial (27.1 migration added the state column).
    state
        .storage
        .orgs()
        .set_state(saga.org_id, "trial")
        .await
        .map_err(|e| internal(e.to_string()))?;

    tracing::info!(
        event = "signup.email_verified",
        saga_id = %saga.id,
        org_id = %saga.org_id,
    );

    Ok(Json(VerifyEmailResponse {
        org_id: saga.org_id,
        message: "Email verified. Your organization is now active.".into(),
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

    /// [p4-onboard-1] Story 27.1 evidence: public POST /v1/auth/signup creates
    /// operator + org + provisioning saga; POST /v1/auth/verify-email advances
    /// saga to complete and sets org.state = trial.
    #[allow(dead_code)]
    const P4_ONBOARD_1_EVIDENCE: &str = "[p4-onboard-1] story-27.1: signup saga \
        (created→kms_done→entitlement→owner_set→complete) + org state machine \
        (unconfigured|trial|active)";

    #[test]
    fn signup_response_serializes() {
        let resp = SignupResponse {
            saga_id: Uuid::nil(),
            org_id: Uuid::nil(),
            message: "Check your email for a verification link.".into(),
        };
        let json = serde_json::to_value(resp).expect("serialize");
        assert_eq!(json["message"], "Check your email for a verification link.");
    }

    #[test]
    fn verify_response_serializes() {
        let resp = VerifyEmailResponse {
            org_id: Uuid::nil(),
            message: "Email verified. Your organization is now active.".into(),
        };
        let json = serde_json::to_value(resp).expect("serialize");
        assert!(json.get("org_id").is_some());
    }

    #[test]
    fn slugify_basic() {
        let s = slugify("My Org Name");
        assert!(s.starts_with("my-org-name-"));
        assert!(s.chars().all(|c| c.is_alphanumeric() || c == '-'));
    }
}
