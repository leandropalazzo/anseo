//! Operator entity-admin API — Story 48.4 (anseo OSS core half).
//!
//! Project-agnostic operator surface over the OSS-owned `entities` +
//! `verification_attempts` tables (Epic 43). Reached server-to-server by the
//! anseo-web BFF with the single global operator credential
//! `ANSEO_OPERATOR_API_KEY` (see [`crate::middleware::auth::require_operator_key`]).
//! Tenant project API keys do NOT reach this surface.
//!
//! Endpoints (all under `/v1/operator`):
//!   * `GET  /entities`                         — list/filter/paginate.
//!   * `GET  /entities/:domain`                 — entity + attempts (newest-first).
//!   * `POST /entities/:domain/revoke`          — shared revoke path (43.2 + 48.4).
//!   * `POST /entities/:domain/override-verify` — manual verify with a reason.
//!   * `POST /entities/:domain/retrigger`       — re-issue the 43.2 verification.
//!   * `POST /entities/:domain/erase`           — two-step GDPR erase + KEK shred.
//!
//! ### Erase / KEK safety (operator decision — implemented exactly as specified)
//!
//! `ProjectKek` is project-keyed; entities are domain-keyed. We destroy a KEK
//! (crypto-shred, [`anseo_benchmark::ProjectKek::destroy`]) ONLY where an
//! UNAMBIGUOUS entity→project mapping exists — i.e. exactly one project owns
//! identified contributions for the domain via the identified-contribution
//! linkage (`contributions.entity_domain`, migration 20260606140000).
//! Otherwise we DO NOT guess: the response carries `kek_destroyed: false` with a
//! `kek_skip_reason`. A KEK shared by — or belonging to — unrelated contributors
//! is never destroyed for one domain's erasure. See [`decide_kek_action`].
//!
//! ### Audit
//!
//! The OSS core records the revocation ledger row (shared revoke path) plus the
//! append-only `verification_attempts` rows. The richer operator audit log
//! (actor + before/after into the `anseo_admin` schema) is written by the
//! anseo-web BFF (A5 v2) — it is NOT an OSS-core table, so it is intentionally
//! out of scope here. The actor login is accepted via the
//! `X-Anseo-Operator-Actor` header / `operator` body field and echoed on
//! responses so the BFF can audit it.
//!
//! Dynamic sqlx only (via the repository layer). No `query!` macros.

use std::time::{SystemTime, UNIX_EPOCH};

use anseo_benchmark::ProjectKek;
use anseo_storage::repositories::entities::{
    EntityListFilters, EntityProjectMapping, EntityRecord,
};
use anseo_storage::repositories::verification::{AttemptRecord, VerificationMethod};
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::middleware::auth::{AuthenticatedOperator, OPERATOR_API_KEY_ENV};
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;
type ApiError = (StatusCode, Json<serde_json::Value>);

/// Default page size for the list endpoint when `limit` is absent.
const DEFAULT_LIMIT: i64 = 50;
/// Hard cap on `limit` so a single page can never pull the whole table.
const MAX_LIMIT: i64 = 200;
/// Confirm-token TTL for the two-step erase (~5 minutes).
const ERASE_TOKEN_TTL_SECS: u64 = 300;

/// Operator sub-router. Mounted at `/v1/operator` behind `require_operator_key`
/// in `lib.rs` — NOT the tenant `require_api_key` / `X-Anseo-Project` gate.
pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/operator/entities", get(list_entities))
        .route("/operator/entities/:domain", get(get_entity))
        .route("/operator/entities/:domain/revoke", post(revoke))
        .route(
            "/operator/entities/:domain/override-verify",
            post(override_verify),
        )
        .route("/operator/entities/:domain/retrigger", post(retrigger))
        .route("/operator/entities/:domain/erase", post(erase))
}

fn storage_err(e: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
    )
}

fn not_found(domain: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "entity_not_found", "domain": domain })),
    )
}

fn bad_request(code: &str, msg: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": code, "message": msg })),
    )
}

fn conflict(code: &str, msg: &str) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({ "error": code, "message": msg })),
    )
}

/// Resolve the acting operator: the request `operator` body field wins, else
/// the `X-Anseo-Operator-Actor` header captured by the auth layer, else the
/// `"operator"` sentinel. The login is echoed on responses for BFF auditing.
fn resolve_actor(body_operator: Option<&str>, op: &AuthenticatedOperator) -> String {
    body_operator
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| op.actor.clone())
        .unwrap_or_else(|| "operator".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Views
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EntityView {
    pub domain: String,
    pub display_name: String,
    pub role: String,
    pub claim_status: String,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verification_method: Option<String>,
    pub grace_period_start: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<EntityRecord> for EntityView {
    fn from(r: EntityRecord) -> Self {
        Self {
            domain: r.domain,
            display_name: r.display_name,
            role: r.role,
            claim_status: r.claim_status,
            verified_at: r.verified_at,
            verification_method: r.verification_method,
            grace_period_start: r.grace_period_start,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AttemptView {
    pub id: uuid::Uuid,
    pub method: String,
    pub state: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub consumed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<AttemptRecord> for AttemptView {
    fn from(a: AttemptRecord) -> Self {
        Self {
            id: a.id,
            method: a.method,
            state: a.state,
            expires_at: a.expires_at,
            consumed_at: a.consumed_at,
            created_at: a.created_at,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/operator/entities
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub claim_status: Option<String>,
    pub verification_method: Option<String>,
    /// Case-insensitive substring of the domain.
    pub domain: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub entities: Vec<EntityView>,
    pub limit: i64,
    pub offset: i64,
    pub count: usize,
}

/// Clamp a requested limit into `[1, MAX_LIMIT]`, defaulting when absent.
fn clamp_limit(requested: Option<i64>) -> i64 {
    requested.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Clamp a requested offset to be non-negative.
fn clamp_offset(requested: Option<i64>) -> i64 {
    requested.unwrap_or(0).max(0)
}

async fn list_entities(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ListResponse>, ApiError> {
    let limit = clamp_limit(q.limit);
    let offset = clamp_offset(q.offset);
    let filters = EntityListFilters {
        claim_status: q.claim_status.filter(|s| !s.trim().is_empty()),
        verification_method: q.verification_method.filter(|s| !s.trim().is_empty()),
        domain: q.domain.filter(|s| !s.trim().is_empty()),
    };
    let rows = state
        .storage
        .entities()
        .list(&filters, limit, offset)
        .await
        .map_err(storage_err)?;
    let entities: Vec<EntityView> = rows.into_iter().map(EntityView::from).collect();
    Ok(Json(ListResponse {
        count: entities.len(),
        entities,
        limit,
        offset,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/operator/entities/:domain
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EntityDetail {
    #[serde(flatten)]
    pub entity: EntityView,
    pub verification_attempts: Vec<AttemptView>,
}

async fn get_entity(
    Path(raw_domain): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<EntityDetail>, ApiError> {
    let domain = anseo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);
    let entity = state
        .storage
        .entities()
        .get(&domain)
        .await
        .map_err(storage_err)?
        .ok_or_else(|| not_found(&domain))?;
    let attempts = state
        .storage
        .verification()
        .attempts_for_domain(&domain)
        .await
        .map_err(storage_err)?;
    Ok(Json(EntityDetail {
        entity: EntityView::from(entity),
        verification_attempts: attempts.into_iter().map(AttemptView::from).collect(),
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/operator/entities/:domain/revoke
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct OperatorActorBody {
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

async fn revoke(
    Path(raw_domain): Path<String>,
    State(state): State<AppState>,
    Extension(op): Extension<AuthenticatedOperator>,
    body: Option<Json<OperatorActorBody>>,
) -> Result<Json<EntityView>, ApiError> {
    let domain = anseo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let actor = resolve_actor(body.operator.as_deref(), &op);

    // 404 unless the entity exists — never silently "revoke" a non-entity.
    let entity = state
        .storage
        .entities()
        .get(&domain)
        .await
        .map_err(storage_err)?
        .ok_or_else(|| not_found(&domain))?;

    // Only a currently-`verified` entity is revocable. `set_grace_period_start`
    // is a no-op on any other state (its WHERE clause), which would otherwise
    // write a misleading revocation ledger row and return 200 on an unchanged
    // entity. Reject up front with 409 so the operator gets an honest answer.
    if entity.claim_status != "verified" {
        return Err(conflict(
            "entity_not_revocable",
            &format!(
                "cannot revoke entity in '{}' state; only 'verified' entities can be revoked",
                entity.claim_status
            ),
        ));
    }

    // SHARED revoke path: the exact same `revoke_entity` the daily re-verify job
    // uses (set revoked + grace-period start + ledger row). One revoke path.
    anseo_storage::repositories::verification::revoke_entity(&state.storage, &domain)
        .await
        .map_err(storage_err)?;

    tracing::info!(event = "operator.entity_revoked", domain = %domain, actor = %actor);
    reload(&state, &domain).await
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/operator/entities/:domain/override-verify
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OverrideVerifyBody {
    /// Required, non-empty: the recorded reason for the manual override.
    pub reason: String,
    #[serde(default)]
    pub operator: Option<String>,
}

async fn override_verify(
    Path(raw_domain): Path<String>,
    State(state): State<AppState>,
    Extension(op): Extension<AuthenticatedOperator>,
    Json(body): Json<OverrideVerifyBody>,
) -> Result<Json<EntityView>, ApiError> {
    let domain = anseo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);
    if body.reason.trim().is_empty() {
        return Err(bad_request(
            "reason_required",
            "override-verify requires a non-empty reason",
        ));
    }
    let actor = resolve_actor(body.operator.as_deref(), &op);

    let _ = state
        .storage
        .entities()
        .get(&domain)
        .await
        .map_err(storage_err)?
        .ok_or_else(|| not_found(&domain))?;

    // Mark verified with the manual-override method (distinct from the
    // self-service dns_txt / email_magic_link methods — see migration
    // 20260609100000). The reason is carried to the BFF for the audit log.
    state
        .storage
        .entities()
        .set_claim_status(&domain, "verified", Some("manual_override"))
        .await
        .map_err(storage_err)?;

    tracing::info!(
        event = "operator.entity_override_verified",
        domain = %domain,
        actor = %actor,
        reason = %body.reason.trim(),
    );
    reload(&state, &domain).await
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/operator/entities/:domain/retrigger
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RetriggerBody {
    /// Role address to re-send the magic link to (required: retrigger reuses
    /// the email_magic_link path from 43.2).
    pub email: String,
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RetriggerResponse {
    pub domain: String,
    pub state: String,
    pub method: String,
    pub email_sent_to: String,
}

async fn retrigger(
    Path(raw_domain): Path<String>,
    State(state): State<AppState>,
    Extension(op): Extension<AuthenticatedOperator>,
    Json(body): Json<RetriggerBody>,
) -> Result<Json<RetriggerResponse>, ApiError> {
    let domain = anseo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);
    let email = body.email.trim().to_string();
    if email.is_empty() {
        return Err(bad_request(
            "email_required",
            "retrigger requires the role address to re-send the magic link to",
        ));
    }
    let actor = resolve_actor(body.operator.as_deref(), &op);

    let _ = state
        .storage
        .entities()
        .get(&domain)
        .await
        .map_err(storage_err)?
        .ok_or_else(|| not_found(&domain))?;

    // Reuse the 43.2 mint path: expire prior live email challenges, mint a new
    // one, and re-send via the comms SMTP wire. Honest failure if mail is not
    // configured (no silent success).
    let verification = state.storage.verification();
    verification
        .expire_live_challenges(&domain, VerificationMethod::EmailMagicLink)
        .await
        .map_err(storage_err)?;
    let challenge = verification
        .create_challenge(
            &domain,
            VerificationMethod::EmailMagicLink,
            None,
            Some(&email),
        )
        .await
        .map_err(storage_err)?;

    match crate::routes::verification::send_magic_link_for_operator(
        &state,
        &email,
        &challenge.raw_token,
    )
    .await
    {
        Ok(()) => {
            tracing::info!(event = "operator.entity_retriggered", domain = %domain, actor = %actor);
            Ok(Json(RetriggerResponse {
                domain,
                state: "pending".to_string(),
                method: VerificationMethod::EmailMagicLink.as_str().to_string(),
                email_sent_to: email,
            }))
        }
        Err(detail) => {
            // The challenge is persisted (DNS-TXT fallback / retry stay usable),
            // but mail did NOT send — surface a 502 so the operator knows.
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "email_dispatch_failed",
                    "message": format!(
                        "verification challenge created, but the magic-link email could \
                         not be sent ({detail}); SMTP may not be configured"
                    ),
                    "domain": domain,
                })),
            ))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/operator/entities/:domain/erase  (two-step confirm + KEK shred)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
pub struct EraseBody {
    /// Confirm token from the first (token-less) call. When absent, the call
    /// returns a fresh short-lived token and erases NOTHING.
    #[serde(default)]
    pub confirm_token: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EraseChallenge {
    pub domain: String,
    pub confirm_required: bool,
    /// Short-lived (~5 min) signed token bound to (domain, actor). Present it on
    /// the second call to actually erase. No new storage backs this — it is a
    /// self-verifying HMAC.
    pub confirm_token: String,
    pub expires_in_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct EraseResult {
    pub domain: String,
    pub erased: bool,
    pub entity_rows: u64,
    pub attempt_rows: u64,
    pub dispute_rows: u64,
    /// True only when an UNAMBIGUOUS entity→project mapping existed and the KEK
    /// was crypto-shredded. False otherwise (with `kek_skip_reason`).
    pub kek_destroyed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kek_skip_reason: Option<String>,
}

/// Untagged so the two-step shape serializes cleanly: a token-less call returns
/// an [`EraseChallenge`]; a confirmed call returns an [`EraseResult`].
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EraseResponse {
    Challenge(EraseChallenge),
    Result(EraseResult),
}

/// The KEK crypto-shred decision (pure; unit-tested). Given the resolved
/// entity→project mapping, decide whether to destroy a KEK and, if not, why.
///
/// SAFETY: destroy ONLY for an unambiguous single-project mapping. Never guess
/// when zero or multiple projects could be affected.
pub fn decide_kek_action(mapping: &EntityProjectMapping) -> KekAction {
    match mapping {
        EntityProjectMapping::Unique(project_id) => KekAction::Destroy {
            project_id: project_id.clone(),
        },
        EntityProjectMapping::None => KekAction::Skip {
            reason: "no identified contribution links this domain to a project, \
                     so there is no project KEK to crypto-shred"
                .to_string(),
        },
        EntityProjectMapping::Ambiguous { project_ids } => KekAction::Skip {
            reason: format!(
                "domain maps to {} projects ({}); destroying any one KEK could \
                 shred unrelated contributors — refusing to guess",
                project_ids.len(),
                project_ids.join(", ")
            ),
        },
    }
}

/// Outcome of [`decide_kek_action`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KekAction {
    Destroy { project_id: String },
    Skip { reason: String },
}

async fn erase(
    Path(raw_domain): Path<String>,
    State(state): State<AppState>,
    Extension(op): Extension<AuthenticatedOperator>,
    body: Option<Json<EraseBody>>,
) -> Result<Json<EraseResponse>, ApiError> {
    let domain = anseo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);
    let body = body.map(|Json(b)| b).unwrap_or_default();
    let actor = resolve_actor(body.operator.as_deref(), &op);

    // Entity must exist to erase / to issue a confirm token.
    let _ = state
        .storage
        .entities()
        .get(&domain)
        .await
        .map_err(storage_err)?
        .ok_or_else(|| not_found(&domain))?;

    let signing_key = operator_signing_key()?;

    // Step 1 (no token): return a short-lived signed confirm token. Erase
    // NOTHING. The token binds (domain, actor) so a token minted for one
    // domain/actor cannot confirm a different one.
    let Some(token) = body
        .confirm_token
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    else {
        let now = unix_now();
        let confirm_token = mint_confirm_token(&signing_key, &domain, &actor, now);
        return Ok(Json(EraseResponse::Challenge(EraseChallenge {
            domain,
            confirm_required: true,
            confirm_token,
            expires_in_secs: ERASE_TOKEN_TTL_SECS,
        })));
    };

    // Step 2: verify the token before destroying anything.
    if !verify_confirm_token(&signing_key, token, &domain, &actor, unix_now()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "invalid_confirm_token",
                "message": "the confirm token is missing, expired, or not bound to this \
                            (domain, operator); request a fresh token by calling erase \
                            without a confirm_token",
            })),
        ));
    }

    // Resolve the entity→project mapping FIRST, then decide the KEK action. The
    // row delete and the KEK shred are distinct: rows are deleted regardless,
    // the KEK is destroyed ONLY for an unambiguous single-project mapping.
    let mapping = state
        .storage
        .entities()
        .project_for_domain(&domain)
        .await
        .map_err(storage_err)?;
    let kek_action = decide_kek_action(&mapping);

    // Transactional row delete (entity + attempts + identifiable disputes).
    let counts = state
        .storage
        .entities()
        .erase(&domain)
        .await
        .map_err(storage_err)?;

    let (kek_destroyed, kek_skip_reason) = match kek_action {
        KekAction::Destroy { project_id } => {
            let store = anseo_core::default_chain();
            match ProjectKek::destroy(&store, &project_id) {
                Ok(()) => {
                    tracing::warn!(
                        event = "operator.entity_kek_shredded",
                        domain = %domain,
                        project_id = %project_id,
                        actor = %actor,
                        "crypto-shredded project KEK for unambiguous entity→project mapping"
                    );
                    (true, None)
                }
                Err(e) => {
                    // Rows are already erased; report the KEK failure honestly
                    // rather than claiming a shred that did not happen.
                    tracing::error!(
                        event = "operator.entity_kek_shred_failed",
                        domain = %domain, project_id = %project_id, error = %e,
                    );
                    (false, Some(format!("KEK destroy failed: {e}")))
                }
            }
        }
        KekAction::Skip { reason } => {
            tracing::info!(
                event = "operator.entity_kek_skipped",
                domain = %domain, actor = %actor, reason = %reason,
            );
            (false, Some(reason))
        }
    };

    Ok(Json(EraseResponse::Result(EraseResult {
        domain,
        erased: true,
        entity_rows: counts.entity_rows,
        attempt_rows: counts.attempt_rows,
        dispute_rows: counts.dispute_rows,
        kek_destroyed,
        kek_skip_reason,
    })))
}

// ─────────────────────────────────────────────────────────────────────────────
// Confirm-token HMAC (no new storage)
// ─────────────────────────────────────────────────────────────────────────────

/// Signing key for the erase confirm token. Derived from the operator
/// credential so it is server-side-only and rotates with the credential. The
/// operator surface is unreachable without `ANSEO_OPERATOR_API_KEY` configured
/// (the auth gate 503s first), so this is always present on the erase path.
fn operator_signing_key() -> Result<Vec<u8>, ApiError> {
    std::env::var(OPERATOR_API_KEY_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(|v| v.into_bytes())
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "operator_surface_disabled",
                    "message": "ANSEO_OPERATOR_API_KEY is not configured",
                })),
            )
        })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `hex(HMAC(key, "domain|actor|expiry")) . expiry`. Self-verifying: the
/// expiry travels in cleartext but is signed, so it cannot be extended without
/// the key. Bound to (domain, actor) so a token cannot be replayed elsewhere.
fn mint_confirm_token(key: &[u8], domain: &str, actor: &str, now: u64) -> String {
    let expiry = now + ERASE_TOKEN_TTL_SECS;
    let mac = sign(key, domain, actor, expiry);
    format!("{mac}.{expiry}")
}

fn verify_confirm_token(key: &[u8], token: &str, domain: &str, actor: &str, now: u64) -> bool {
    let Some((mac_hex, expiry_str)) = token.rsplit_once('.') else {
        return false;
    };
    let Ok(expiry) = expiry_str.parse::<u64>() else {
        return false;
    };
    if expiry < now {
        return false; // expired
    }
    let expected = sign(key, domain, actor, expiry);
    // Constant-time compare via subtle (mac strings are equal length on match).
    crate::middleware::auth::ct_eq_str(&expected, mac_hex)
}

fn sign(key: &[u8], domain: &str, actor: &str, expiry: u64) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(domain.as_bytes());
    mac.update(b"|");
    mac.update(actor.as_bytes());
    mac.update(b"|");
    mac.update(expiry.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared
// ─────────────────────────────────────────────────────────────────────────────

/// Re-read the entity after a mutation so the response reflects the new state.
async fn reload(state: &AppState, domain: &str) -> Result<Json<EntityView>, ApiError> {
    let e = state
        .storage
        .entities()
        .get(domain)
        .await
        .map_err(storage_err)?
        .ok_or_else(|| not_found(domain))?;
    Ok(Json(EntityView::from(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_benchmark::kek_secret_key;
    use anseo_core::{InMemoryStore, Secret, SecretStore as _};

    #[test]
    fn limit_offset_clamping() {
        assert_eq!(clamp_limit(None), DEFAULT_LIMIT);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(-5)), 1);
        assert_eq!(clamp_limit(Some(10_000)), MAX_LIMIT);
        assert_eq!(clamp_limit(Some(25)), 25);
        assert_eq!(clamp_offset(None), 0);
        assert_eq!(clamp_offset(Some(-1)), 0);
        assert_eq!(clamp_offset(Some(40)), 40);
    }

    #[test]
    fn actor_resolution_precedence() {
        let op = AuthenticatedOperator {
            actor: Some("from-header".into()),
        };
        // Body wins over header.
        assert_eq!(resolve_actor(Some("from-body"), &op), "from-body");
        // Empty body falls back to header.
        assert_eq!(resolve_actor(Some("  "), &op), "from-header");
        assert_eq!(resolve_actor(None, &op), "from-header");
        // No header → sentinel.
        let none = AuthenticatedOperator { actor: None };
        assert_eq!(resolve_actor(None, &none), "operator");
    }

    #[test]
    fn confirm_token_round_trips_and_is_bound() {
        let key = b"operator-secret-key";
        let now = 1_000_000u64;
        let tok = mint_confirm_token(key, "example.com", "alice", now);
        // Valid for the exact (domain, actor).
        assert!(verify_confirm_token(key, &tok, "example.com", "alice", now));
        // Bound to domain: different domain rejected.
        assert!(!verify_confirm_token(key, &tok, "other.com", "alice", now));
        // Bound to actor: different actor rejected.
        assert!(!verify_confirm_token(key, &tok, "example.com", "bob", now));
        // Expired (now past expiry) rejected.
        assert!(!verify_confirm_token(
            key,
            &tok,
            "example.com",
            "alice",
            now + ERASE_TOKEN_TTL_SECS + 1
        ));
        // Different signing key rejected (no forgery).
        assert!(!verify_confirm_token(
            b"different-key",
            &tok,
            "example.com",
            "alice",
            now
        ));
        // Garbage token rejected.
        assert!(!verify_confirm_token(
            key,
            "not-a-token",
            "example.com",
            "alice",
            now
        ));
    }

    #[test]
    fn kek_action_destroys_only_for_unique_mapping() {
        // Unique → destroy that project's KEK.
        let act = decide_kek_action(&EntityProjectMapping::Unique("proj-1".into()));
        assert_eq!(
            act,
            KekAction::Destroy {
                project_id: "proj-1".into()
            }
        );

        // None → skip, no KEK to destroy.
        let act = decide_kek_action(&EntityProjectMapping::None);
        assert!(matches!(act, KekAction::Skip { .. }));

        // Ambiguous → skip, never guess.
        let act = decide_kek_action(&EntityProjectMapping::Ambiguous {
            project_ids: vec!["a".into(), "b".into()],
        });
        match act {
            KekAction::Skip { reason } => assert!(reason.contains("2 projects")),
            other => panic!("expected Skip, got {other:?}"),
        }
    }

    #[test]
    fn kek_destroy_only_runs_for_unique_mapping_against_store() {
        // Mirror crypto.rs: prove the destroy is gated by the decision. A
        // unique mapping shreds exactly its project's KEK; an ambiguous mapping
        // shreds nothing.
        let store = InMemoryStore::durable_for_tests();
        store
            .set(&kek_secret_key("proj-1"), Secret::new("a".repeat(64)))
            .unwrap();
        store
            .set(&kek_secret_key("proj-2"), Secret::new("b".repeat(64)))
            .unwrap();

        // Unique(proj-1): destroy only proj-1.
        if let KekAction::Destroy { project_id } =
            decide_kek_action(&EntityProjectMapping::Unique("proj-1".into()))
        {
            ProjectKek::destroy(&store, &project_id).unwrap();
        } else {
            panic!("expected Destroy");
        }
        assert!(store.get(&kek_secret_key("proj-1")).is_err());
        assert!(store.get(&kek_secret_key("proj-2")).is_ok());

        // Ambiguous: skip → nothing else destroyed.
        let act = decide_kek_action(&EntityProjectMapping::Ambiguous {
            project_ids: vec!["proj-2".into(), "proj-3".into()],
        });
        assert!(matches!(act, KekAction::Skip { .. }));
        assert!(store.get(&kek_secret_key("proj-2")).is_ok());
    }
}
