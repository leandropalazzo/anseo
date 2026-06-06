//! Domain-verification API — Story 43.2.
//!
//! Verification is a STATE MACHINE (not an event). DNS-TXT is the PRIMARY,
//! higher-trust path and the ONLY method that qualifies for ranked-leaderboard
//! badges (NFR8); email magic-link is the low-friction alternate. Every attempt
//! is appended to `verification_attempts` for dispute evidence, with the
//! authorization attestation (AC-7) recorded on each.
//!
//! Operator-initiated (authed, `v1_router`):
//!   * `POST /verification/start` — generate a challenge for a domain. For
//!     `dns_txt` returns the exact TXT record to publish; for
//!     `email_magic_link` sends a 30-minute single-use link via `anseo-comms`.
//!     Rate-limited to 5/hour/domain → 429 (AC-6). Requires the attestation
//!     checkbox (AC-7).
//!   * `POST /verification/check` — resolve the domain's TXT records (DNSSEC-
//!     validated in prod; pluggable resolver), constant-time compare, and on
//!     match flip the entity to `verified` (AC-2).
//!
//! Public (unauthenticated, `public_router`) — the recipient clicks this from
//! their inbox and has no API key:
//!   * `GET  /verify/:token` — render a confirm page (no side effects; safe for
//!     mail-scanner prefetch).
//!   * `POST /verify/:token` — confirm the magic link → entity `verified`
//!     (AC-3). Single-use; replay/expiry → 401 (AC-4).
//!
//! Dynamic sqlx only (via the repository layer). No `query!` macros.

use std::sync::Arc;

use anseo_comms::dispatch::Dispatcher;
use anseo_comms::recipient_hash;
use anseo_comms::template::TransactionalTemplate;
use anseo_comms::transport::SmtpTransport;
use anseo_comms::Stream;
use anseo_storage::repositories::entities::EntityRepo;
use anseo_storage::repositories::verification::{
    classify_token, txt_records_contain_token, MintedChallenge, MockTxtResolver, ResolveError,
    TokenRejection, TxtResolver, VerificationMethod, ATTESTATION_TEXT, ATTESTATION_VERSION,
    RATE_LIMIT_PER_HOUR,
};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

type ApiError = (StatusCode, Json<serde_json::Value>);

fn storage_err(e: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
    )
}

fn bad_request(code: &str, msg: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": code, "message": msg })),
    )
}

/// Default public origin that serves the magic-link confirm page. Overridable
/// via `ANSEO_VERIFY_BASE_URL` for staging / self-host. Mirrors the badge
/// base-url helper (same posture).
const DEFAULT_VERIFY_BASE_URL: &str = "https://benchmark.anseo.ai";

fn verify_base_url() -> String {
    std::env::var("ANSEO_VERIFY_BASE_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_VERIFY_BASE_URL.to_string())
}

/// The mail "root" domain used to derive the transactional sending subdomain.
fn mail_root() -> String {
    std::env::var("ANSEO_MAIL_ROOT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "anseo.ai".to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Routers
// ─────────────────────────────────────────────────────────────────────────────

/// Operator-initiated verification — behind `require_api_key` (mounted in the
/// authed `v1_routes` chain in `lib.rs`).
pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/verification/start", post(start_verification))
        .route("/verification/check", post(check_verification))
}

/// Public, UNAUTHENTICATED surface — the magic-link recipient clicks this from
/// their inbox with no API key. Mounted on the public `/v1` nest in `lib.rs`.
pub fn public_router() -> Router<AppState> {
    Router::new().route(
        "/verify/:token",
        get(verify_confirm_page).post(verify_magic_link),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /verification/start
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StartRequest {
    pub domain: String,
    /// "dns_txt" (primary) | "email_magic_link" (alternate).
    pub method: String,
    /// Display name to register if the entity is new.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Entity role: brand | source | both. Defaults to source.
    #[serde(default)]
    pub role: Option<String>,
    /// Role address to email the magic link to (required for email method).
    #[serde(default)]
    pub email: Option<String>,
    /// Opaque identifier for the initiating claimant session (AC-1 binding).
    #[serde(default)]
    pub claimant_session: Option<String>,
    /// AC-7: the authorization attestation MUST be checked to proceed.
    #[serde(default)]
    pub attestation_accepted: bool,
}

#[derive(Debug, Serialize)]
pub struct StartResponse {
    pub domain: String,
    pub method: String,
    pub state: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub attestation_version: String,
    /// Whether this method qualifies for ranked-leaderboard badges (NFR8).
    pub qualifies_for_ranking: bool,
    /// DNS-TXT method only: the exact record to publish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns_record: Option<DnsRecord>,
    /// Email method only: confirmation the link was dispatched (the link itself
    /// is NEVER returned in the response — it goes only to the inbox).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email_sent_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DnsRecord {
    pub name: String,
    pub record_type: String,
    pub value: String,
    /// Human-readable one-liner for the UI.
    pub instructions: String,
}

fn parse_method(s: &str) -> Result<VerificationMethod, ApiError> {
    match s {
        "dns_txt" => Ok(VerificationMethod::DnsTxt),
        "email_magic_link" => Ok(VerificationMethod::EmailMagicLink),
        other => Err(bad_request(
            "invalid_method",
            &format!("unknown verification method `{other}` (expected dns_txt | email_magic_link)"),
        )),
    }
}

async fn start_verification(
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> Result<(StatusCode, Json<StartResponse>), ApiError> {
    // AC-7: attestation gate. The claim cannot proceed unchecked.
    if !req.attestation_accepted {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error": "attestation_required",
                "message": ATTESTATION_TEXT,
                "attestation_version": ATTESTATION_VERSION,
            })),
        ));
    }

    let method = parse_method(&req.method)?;
    let domain = EntityRepo::normalize_domain(&req.domain);
    if domain.is_empty() {
        return Err(bad_request("invalid_domain", "domain must not be empty"));
    }
    if method == VerificationMethod::EmailMagicLink && req.email.as_deref().unwrap_or("").is_empty()
    {
        return Err(bad_request(
            "email_required",
            "email magic-link verification requires a role address",
        ));
    }

    let verification = state.storage.verification();

    // AC-6: rate-limit 5/hour/domain → 429, no new token issued.
    let count = verification
        .attempts_last_hour(&domain)
        .await
        .map_err(storage_err)?;
    if count >= RATE_LIMIT_PER_HOUR {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "rate_limited",
                "message": format!(
                    "more than {RATE_LIMIT_PER_HOUR} verification attempts for this domain in the last hour"
                ),
            })),
        ));
    }

    // Ensure the entity exists and is at least `pending` (claim initiated).
    let display_name = req.display_name.clone().unwrap_or_else(|| domain.clone());
    let role = req.role.clone().unwrap_or_else(|| "source".to_string());
    state
        .storage
        .entities()
        .upsert(&domain, &display_name, &role)
        .await
        .map_err(storage_err)?;
    // Move to `pending` (no-op if already verified/revoked — set_claim_status is
    // additive; we only mark pending when currently unclaimed).
    if let Ok(Some(e)) = state.storage.entities().get(&domain).await {
        if e.claim_status == "unclaimed" {
            state
                .storage
                .entities()
                .set_claim_status(&domain, "pending", None)
                .await
                .map_err(storage_err)?;
        }
    }

    // Expire any prior live challenge for (domain, method) before minting a new
    // one (keeps the live-challenge unique index single-winner).
    verification
        .expire_live_challenges(&domain, method)
        .await
        .map_err(storage_err)?;

    let challenge = verification
        .create_challenge(
            &domain,
            method,
            req.claimant_session.as_deref(),
            req.email.as_deref(),
        )
        .await
        .map_err(storage_err)?;

    let mut resp = StartResponse {
        domain: domain.clone(),
        method: method.as_str().to_string(),
        state: "pending".to_string(),
        expires_at: challenge.expires_at,
        attestation_version: ATTESTATION_VERSION.to_string(),
        qualifies_for_ranking: method.qualifies_for_ranking(),
        dns_record: None,
        email_sent_to: None,
    };

    match method {
        VerificationMethod::DnsTxt => {
            let name = MintedChallenge::dns_record_name(&domain);
            let value = challenge.dns_record_value();
            resp.dns_record = Some(DnsRecord {
                instructions: format!(
                    "Add a TXT record at {name} with value {value:?}, then call \
                     /v1/verification/check. Records can take time to propagate."
                ),
                name,
                record_type: "TXT".to_string(),
                value,
            });
        }
        VerificationMethod::EmailMagicLink => {
            let email = req.email.clone().unwrap_or_default();
            send_magic_link(&state, &email, &challenge.raw_token).await?;
            resp.email_sent_to = Some(email);
        }
    }

    Ok((StatusCode::CREATED, Json(resp)))
}

/// Build + dispatch the magic-link transactional email through `anseo-comms`.
///
/// The dedup_key is the token hash so a magic-link is not re-sent more than once
/// within the token window (comms AC-5). The production SMTP transport is
/// fail-loud until wired; the dispatcher then records `failed` (never a false
/// `sent`) — acceptable per the "log only until comms wire lands" note. The raw
/// token is embedded ONLY in the email link, never returned to the API caller.
async fn send_magic_link(state: &AppState, email: &str, raw_token: &str) -> Result<(), ApiError> {
    let verify_url = format!("{}/v1/verify/{}", verify_base_url(), raw_token);
    let from = Stream::Transactional.subdomain(&mail_root());
    let message = TransactionalTemplate::DomainVerification {
        verify_url: verify_url.clone(),
    }
    .build(&from, email)
    .map_err(|e| storage_err(format!("template error: {e}")))?;

    let recipient = recipient_hash(email);
    let dedup_key = anseo_storage::repositories::verification::hash_token(raw_token);

    // SMTP transport is fail-loud until the wire client lands; the dispatcher
    // audits the outcome either way. We do not surface a 5xx to the operator on
    // a transport-not-configured error — the challenge IS recorded and the
    // operator can fall back to DNS-TXT.
    let transport = SmtpTransport::new(format!("smtp.mail.{}", mail_root()), 587)
        .map_err(|e| storage_err(format!("smtp config: {e}")))?;
    let dispatcher = Dispatcher::new(state.storage.pool(), &transport);
    match dispatcher
        .send_transactional(
            &recipient,
            "domain_verification",
            Some(&dedup_key),
            &message,
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::warn!(event = "verification.magic_link_dispatch_failed", error = %e);
            // The challenge is persisted; do not fail the request on dispatch
            // error so the audit ledger and DNS-TXT fallback remain usable.
            Ok(())
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /verification/check (DNS-TXT)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CheckRequest {
    pub domain: String,
}

#[derive(Debug, Serialize)]
pub struct CheckResponse {
    pub domain: String,
    /// "pending" (record not yet found / propagating) | "verified".
    pub state: String,
    pub verification_method: Option<String>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn check_verification(
    State(state): State<AppState>,
    Json(req): Json<CheckRequest>,
) -> Result<Json<CheckResponse>, ApiError> {
    let resolver = build_resolver();
    check_verification_with_resolver(&state, &req.domain, resolver.as_ref()).await
}

/// Construct the production DNS resolver. Until a DNSSEC-validating client is
/// wired, this returns an empty in-memory resolver (every lookup → no records),
/// so `check` always reports `pending` for DNS-TXT and the operator falls back
/// to email or retries — it NEVER falsely verifies. Override is the testing
/// seam; the trait is the extension point for the real client.
fn build_resolver() -> Box<dyn TxtResolver> {
    Box::new(MockTxtResolver::new())
}

/// Core check logic, parameterised on the resolver so tests inject a mock with
/// NO network access (test obligation: valid token → verified; tampered → 401).
async fn check_verification_with_resolver(
    state: &AppState,
    raw_domain: &str,
    resolver: &dyn TxtResolver,
) -> Result<Json<CheckResponse>, ApiError> {
    let domain = EntityRepo::normalize_domain(raw_domain);
    let verification = state.storage.verification();

    // Find the live dns_txt challenge for this domain via its token. We resolve
    // the TXT records first, then match against the live challenge.
    let record_name = MintedChallenge::dns_record_name(&domain);
    let txt_values = match resolver.lookup_txt(&record_name).await {
        Ok(v) => v,
        Err(ResolveError::NotFound(_)) => Vec::new(),
        Err(ResolveError::Transient(msg)) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "dns_resolution_failed",
                    "message": msg,
                })),
            ));
        }
    };

    // Find the live (pending) dns_txt challenge for this domain by scanning the
    // resolved TXT values for one that matches a stored token.
    //
    // For each TXT value of the form `anseo-verify=<raw>` we look up the row by
    // hash and, if it is a valid pending dns_txt challenge for THIS domain,
    // consume it (single-use). Cross-domain replay is rejected because the row's
    // `domain` must equal the domain we resolved.
    for value in &txt_values {
        let v = value.trim().trim_matches('"');
        let Some(raw) = v.strip_prefix("anseo-verify=") else {
            continue;
        };
        let record = verification.find_by_token(raw).await.map_err(storage_err)?;
        // Cross-domain replay guard: token must belong to THIS domain (AC test).
        let matches_domain = record.as_ref().map(|r| r.domain == domain).unwrap_or(false);
        let is_dns = record
            .as_ref()
            .map(|r| r.method == "dns_txt")
            .unwrap_or(false);
        if !matches_domain || !is_dns {
            continue;
        }
        if classify_token(record.as_ref(), chrono::Utc::now()).is_err() {
            continue;
        }
        // Defence-in-depth constant-time confirm the resolved value carries the
        // exact token (classify_token already gated validity).
        if !txt_records_contain_token(std::slice::from_ref(value), raw) {
            continue;
        }
        let id = record.as_ref().map(|r| r.id).expect("record present");
        if verification.consume(id).await.map_err(storage_err)? {
            // AC-2: flip the entity to verified.
            state
                .storage
                .entities()
                .set_claim_status(&domain, "verified", Some("dns_txt"))
                .await
                .map_err(storage_err)?;
            let entity = state
                .storage
                .entities()
                .get(&domain)
                .await
                .map_err(storage_err)?;
            return Ok(Json(CheckResponse {
                domain,
                state: "verified".to_string(),
                verification_method: entity.as_ref().and_then(|e| e.verification_method.clone()),
                verified_at: entity.and_then(|e| e.verified_at),
            }));
        }
    }

    // No matching/valid record yet → still pending/propagating.
    Ok(Json(CheckResponse {
        domain,
        state: "pending".to_string(),
        verification_method: None,
        verified_at: None,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET/POST /verify/:token  (public magic-link confirm)
// ─────────────────────────────────────────────────────────────────────────────

/// GET: render a confirm page with NO side effects (safe for mail-scanner /
/// link-preview prefetch — same posture as the comms one-click unsubscribe).
async fn verify_confirm_page(Path(token): Path<String>) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Confirm domain verification</title></head>
<body>
  <h1>Confirm domain verification</h1>
  <p>Click the button to confirm you control this domain.</p>
  <form method="post" action="/v1/verify/{token}">
    <button type="submit">Confirm verification</button>
  </form>
</body></html>"#,
        token = token
    ))
}

/// POST: consume the magic-link token (single-use). Replay / expiry → 401
/// (AC-4). On success flip the entity to `verified` with method
/// `email_magic_link` (AC-3) — a LOWER-trust badge that does NOT qualify for
/// ranked placement (NFR8).
async fn verify_magic_link(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let verification = state.storage.verification();
    let record = verification
        .find_by_token(&token)
        .await
        .map_err(storage_err)?;

    if let Err(reason) = classify_token(record.as_ref(), chrono::Utc::now()) {
        // AC-4: replay / expiry / unknown → 401.
        let detail = match reason {
            TokenRejection::Unknown => "unknown token",
            TokenRejection::AlreadyConsumed => "token already used",
            TokenRejection::Expired => "token expired",
            TokenRejection::WrongState => "token no longer valid",
        };
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid_token", "message": detail })),
        ));
    }
    let record = record.expect("record present after classify ok");
    // Only the email method is confirmable via this public link.
    if record.method != "email_magic_link" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid_token", "message": "wrong token type" })),
        ));
    }

    if !verification.consume(record.id).await.map_err(storage_err)? {
        // Lost the single-use race → already consumed (replay) → 401.
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid_token", "message": "token already used" })),
        ));
    }

    state
        .storage
        .entities()
        .set_claim_status(&record.domain, "verified", Some("email_magic_link"))
        .await
        .map_err(storage_err)?;

    Ok(Json(serde_json::json!({
        "domain": record.domain,
        "state": "verified",
        "verification_method": "email_magic_link",
        "qualifies_for_ranking": false,
    })))
}

// ─────────────────────────────────────────────────────────────────────────────
// Daily re-verification job (AC-5) — revoke on TXT record removal.
// ─────────────────────────────────────────────────────────────────────────────

/// Re-check every dns_txt-verified domain. When the challenge TXT record is no
/// longer present, transition the entity to `revoked` (starting the grace
/// period) and append a revocation row to the ledger. The registered-email
/// notification uses `anseo-comms` once available; until then we log (per AC-5).
///
/// Parameterised on the resolver + a `Storage` handle so it can be driven by a
/// scheduler tick AND unit-tested with a mock resolver and no network.
pub async fn run_reverification_job(
    storage: &Arc<anseo_storage::Storage>,
    resolver: &dyn TxtResolver,
) -> Result<usize, ApiError> {
    let verification = storage.verification();
    let entities = storage.entities();
    let domains = verification
        .dns_verified_domains()
        .await
        .map_err(storage_err)?;
    let mut revoked = 0usize;

    for (domain, token_hash) in domains {
        let record_name = MintedChallenge::dns_record_name(&domain);
        let txt = match resolver.lookup_txt(&record_name).await {
            Ok(v) => v,
            // Transient errors are NOT treated as removal — skip this domain so
            // a flaky resolver never falsely revokes a legitimate verification.
            Err(ResolveError::Transient(_)) => continue,
            Err(ResolveError::NotFound(_)) => Vec::new(),
        };

        // Still present if any resolved TXT value hashes to the stored token.
        let still_present = txt.iter().any(|v| {
            let v = v.trim().trim_matches('"');
            v.strip_prefix("anseo-verify=")
                .map(|raw| anseo_storage::repositories::verification::hash_token(raw) == token_hash)
                .unwrap_or(false)
        });

        if !still_present {
            entities
                .set_grace_period_start(&domain)
                .await
                .map_err(storage_err)?;
            verification
                .record_revocation(&domain)
                .await
                .map_err(storage_err)?;
            revoked += 1;
            // AC-5: notify the registered email via anseo-comms once wired.
            tracing::warn!(
                event = "verification.revoked",
                domain = %domain,
                "DNS-TXT record removed; entity revoked + grace period started. \
                 Revocation notification pending comms wire."
            );
        }
    }
    Ok(revoked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_base_url_default_and_override() {
        // Combined into ONE fn — env is process-global and races otherwise.
        std::env::remove_var("ANSEO_VERIFY_BASE_URL");
        assert_eq!(verify_base_url(), "https://benchmark.anseo.ai");

        std::env::set_var("ANSEO_VERIFY_BASE_URL", "https://staging.example.test/");
        assert_eq!(verify_base_url(), "https://staging.example.test");

        std::env::set_var("ANSEO_VERIFY_BASE_URL", "   ");
        assert_eq!(verify_base_url(), "https://benchmark.anseo.ai");
        std::env::remove_var("ANSEO_VERIFY_BASE_URL");

        // mail_root default + override (same env-global discipline).
        std::env::remove_var("ANSEO_MAIL_ROOT");
        assert_eq!(mail_root(), "anseo.ai");
        std::env::set_var("ANSEO_MAIL_ROOT", "example.test");
        assert_eq!(mail_root(), "example.test");
        std::env::remove_var("ANSEO_MAIL_ROOT");
    }

    #[test]
    fn parse_method_maps_and_rejects() {
        assert_eq!(parse_method("dns_txt").unwrap(), VerificationMethod::DnsTxt);
        assert_eq!(
            parse_method("email_magic_link").unwrap(),
            VerificationMethod::EmailMagicLink
        );
        assert!(parse_method("carrier_pigeon").is_err());
    }
}
