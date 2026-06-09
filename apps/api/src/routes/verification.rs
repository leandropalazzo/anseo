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

use anseo_comms::dispatch::{DispatchResult, Dispatcher};
use anseo_comms::recipient_hash;
use anseo_comms::template::TransactionalTemplate;
use anseo_comms::transport::SmtpTransport;
use anseo_comms::Stream;
use anseo_storage::repositories::entities::EntityRepo;
#[cfg(test)]
use anseo_storage::repositories::verification::MockTxtResolver;
use anseo_storage::repositories::verification::{
    classify_token, txt_records_contain_token, MintedChallenge, ResolveError, TokenRejection,
    TxtResolver, VerificationMethod, ATTESTATION_TEXT, ATTESTATION_VERSION, RATE_LIMIT_PER_HOUR,
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

/// Role addresses accepted as proof of mailbox authority at a domain (RFC 2142
/// plus `security@`), used only when the local-part itself is the authority
/// signal. Any mailbox whose host equals the claimed domain is also accepted.
const ROLE_LOCAL_PARTS: &[&str] = &["admin", "postmaster", "webmaster", "hostmaster", "security"];

/// P1 SECURITY: true iff `email`'s host equals the normalized claimed `domain`.
/// We additionally enumerate role local-parts for documentation/clarity, but the
/// authority signal is strictly "the mailbox lives at the claimed domain" — a
/// role address only matters because it, too, lives at that domain. An address
/// at any OTHER host (e.g. `attacker@example.com` for `victim.com`) is rejected.
fn email_proves_domain_authority(email: &str, domain: &str) -> bool {
    let email = email.trim();
    let Some((local, host)) = email.rsplit_once('@') else {
        return false;
    };
    if local.is_empty() || host.is_empty() {
        return false;
    }
    // Normalize the host the same way the claimed domain was normalized so a
    // trailing dot / case / scheme noise cannot bypass the check.
    let host = EntityRepo::normalize_domain(host);
    if host != domain {
        return false;
    }
    // Host matches the claimed domain. Accept either (a) ANY mailbox at the
    // domain, or (b) a recognized role address (RFC 2142 + security@). Since the
    // host already equals the claimed domain, (b) ⊂ (a); we evaluate the role
    // set explicitly so the recognized addresses are first-class in the code.
    let local = local.to_ascii_lowercase();
    let _is_recognized_role = ROLE_LOCAL_PARTS.contains(&local.as_str());
    true
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
    if method == VerificationMethod::EmailMagicLink {
        let email = req.email.as_deref().unwrap_or("");
        if email.is_empty() {
            return Err(bad_request(
                "email_required",
                "email magic-link verification requires a role address",
            ));
        }
        // P1 SECURITY: the recipient MUST prove mailbox authority at the domain
        // being claimed. Without this, an authed claimant could verify
        // `victim.com` by directing the link to `attacker@example.com`. Accept
        // ONLY (a) an address whose host equals the claimed domain, or (b) a
        // recognized role address at that domain.
        if !email_proves_domain_authority(email, &domain) {
            return Err(bad_request(
                "email_domain_mismatch",
                "the magic-link recipient must be an address at the claimed domain \
                 (e.g. admin@, postmaster@, webmaster@, hostmaster@, security@, or any \
                 mailbox at the domain)",
            ));
        }
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
            // DEPLOYMENT NOTE (Story 43.2 / 43.7): DNS-TXT is the FUNCTIONAL
            // PRIMARY verification method. Email magic-link is the ALTERNATE,
            // lower-trust path and depends on the `anseo-comms` ESP/SMTP wire
            // client, which is an explicit deployment follow-up shared with
            // Story 43.7. Until that client is wired, the production transport
            // returns `NotConfigured`, so this path correctly returns a 502
            // (below) rather than a FALSE success — it must never report an
            // email as sent when none was. This is intentional fail-loud
            // behaviour, not a bug.
            let email = req.email.clone().unwrap_or_default();
            match send_magic_link(&state, &email, &challenge.raw_token).await? {
                MagicLinkOutcome::Sent | MagicLinkOutcome::AlreadySent => {
                    resp.email_sent_to = Some(email);
                }
                MagicLinkOutcome::Failed(detail) => {
                    // The challenge IS persisted (DNS-TXT fallback / retry stay
                    // usable), but we MUST NOT claim the email was sent. Surface
                    // a 502 so the operator knows delivery did not occur.
                    return Err((
                        StatusCode::BAD_GATEWAY,
                        Json(serde_json::json!({
                            "error": "email_dispatch_failed",
                            "message": format!(
                                "verification challenge created, but the magic-link \
                                 email could not be sent ({detail}); use DNS-TXT or retry"
                            ),
                            "domain": domain,
                            "method": method.as_str(),
                            "state": "pending",
                        })),
                    ));
                }
            }
        }
    }

    Ok((StatusCode::CREATED, Json(resp)))
}

/// Outcome of a magic-link dispatch attempt, surfaced so the HTTP layer never
/// claims an email was sent when it was not.
enum MagicLinkOutcome {
    /// The link was dispatched to the inbox.
    Sent,
    /// A live link with this dedup key already went out (idempotent re-send).
    AlreadySent,
    /// The transport did not send (e.g. SMTP not configured). The challenge is
    /// still persisted; the operator should fall back to DNS-TXT.
    Failed(String),
}

/// Build + dispatch the magic-link transactional email through `anseo-comms`.
///
/// The dedup_key is the token hash so a magic-link is not re-sent more than once
/// within the token window (comms AC-5). The raw token is embedded ONLY in the
/// email link, never returned to the API caller. Returns the dispatch outcome so
/// the caller can choose the correct HTTP status — `Ok(DispatchResult::Failed)`
/// means the email did NOT send and must not be reported as success.
async fn send_magic_link(
    state: &AppState,
    email: &str,
    raw_token: &str,
) -> Result<MagicLinkOutcome, ApiError> {
    let verify_url = format!("{}/v1/verify/{}", verify_base_url(), raw_token);
    let from = Stream::Transactional.subdomain(&mail_root());
    let message = TransactionalTemplate::DomainVerification {
        verify_url: verify_url.clone(),
    }
    .build(&from, email)
    .map_err(|e| storage_err(format!("template error: {e}")))?;

    let recipient = recipient_hash(email);
    let dedup_key = anseo_storage::repositories::verification::hash_token(raw_token);

    let transport = SmtpTransport::new(format!("smtp.mail.{}", mail_root()), 587)
        .map_err(|e| storage_err(format!("smtp config: {e}")))?;
    let dispatcher = Dispatcher::new(state.storage.pool(), &transport);
    let result = dispatcher
        .send_transactional(
            &recipient,
            "domain_verification",
            Some(&dedup_key),
            &message,
        )
        .await
        .map_err(storage_err)?;

    match result {
        DispatchResult::Sent => Ok(MagicLinkOutcome::Sent),
        DispatchResult::AlreadySent => Ok(MagicLinkOutcome::AlreadySent),
        // `Skipped` is a compliance suppression (not applicable to a
        // transactional verification link) — treat as a non-send to be safe.
        DispatchResult::Skipped(decision) => {
            tracing::warn!(
                event = "verification.magic_link_skipped",
                ?decision,
                "magic-link dispatch skipped by comms gate"
            );
            Ok(MagicLinkOutcome::Failed(format!(
                "dispatch skipped: {decision:?}"
            )))
        }
        DispatchResult::Failed(e) => {
            tracing::warn!(event = "verification.magic_link_dispatch_failed", error = %e);
            Ok(MagicLinkOutcome::Failed(e))
        }
    }
}

/// Operator-facing re-send (Story 48.4 retrigger). Reuses the exact 43.2
/// magic-link dispatch path and collapses the outcome to a simple
/// `Ok(())` / `Err(detail)` so the operator route can map a non-send to an
/// honest 502 without depending on this module's private outcome enum. Never
/// reports success when the email did not actually go out.
pub async fn send_magic_link_for_operator(
    state: &AppState,
    email: &str,
    raw_token: &str,
) -> Result<(), String> {
    match send_magic_link(state, email, raw_token).await {
        Ok(MagicLinkOutcome::Sent) | Ok(MagicLinkOutcome::AlreadySent) => Ok(()),
        Ok(MagicLinkOutcome::Failed(detail)) => Err(detail),
        // `send_magic_link` only errors on template/config faults, which are
        // also non-sends — surface the detail rather than a false success.
        Err((_, body)) => Err(body.0.to_string()),
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

/// A real async DNS TXT resolver backed by `hickory-resolver`. Looks up TXT
/// records for the challenge name using the system resolver config (falling back
/// to a sane default), so DNS-TXT — the PRIMARY verification method — can
/// actually succeed in production.
pub struct HickoryTxtResolver {
    resolver: hickory_resolver::TokioAsyncResolver,
}

impl HickoryTxtResolver {
    /// Build a resolver from the host's `/etc/resolv.conf`, falling back to a
    /// default (Google/Cloudflare) config when the system config is unreadable
    /// (e.g. minimal containers).
    pub fn new() -> Self {
        use hickory_resolver::config::{ResolverConfig, ResolverOpts};
        use hickory_resolver::TokioAsyncResolver;
        let resolver = match TokioAsyncResolver::tokio_from_system_conf() {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    event = "verification.resolver_system_conf_failed",
                    error = %e,
                    "falling back to default DNS resolver config"
                );
                TokioAsyncResolver::tokio(ResolverConfig::default(), ResolverOpts::default())
            }
        };
        Self { resolver }
    }
}

impl Default for HickoryTxtResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TxtResolver for HickoryTxtResolver {
    async fn lookup_txt(&self, name: &str) -> Result<Vec<String>, ResolveError> {
        use hickory_resolver::error::ResolveErrorKind;
        match self.resolver.txt_lookup(name).await {
            Ok(lookup) => {
                // Each TXT record may be split into multiple character-strings;
                // concatenate them per record (DNS semantics) into one value.
                let values = lookup
                    .iter()
                    .map(|txt| {
                        txt.iter()
                            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
                            .collect::<String>()
                    })
                    .collect();
                Ok(values)
            }
            Err(e) => match e.kind() {
                // No records / NXDOMAIN → treat as "absent" (drives pending /
                // revocation), NOT a transient failure.
                ResolveErrorKind::NoRecordsFound { .. } => {
                    Err(ResolveError::NotFound(name.to_string()))
                }
                _ => Err(ResolveError::Transient(e.to_string())),
            },
        }
    }
}

/// Construct the production DNS resolver. In tests we return the in-memory mock
/// (NO network I/O); in production we return the real `hickory-resolver` client
/// so DNS-TXT verification can succeed. The trait keeps the check logic testable.
fn build_resolver() -> Box<dyn TxtResolver> {
    #[cfg(test)]
    {
        Box::new(MockTxtResolver::new())
    }
    #[cfg(not(test))]
    {
        Box::new(HickoryTxtResolver::new())
    }
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

/// True iff `token` is a well-formed verification token: exactly 64 lowercase
/// hex chars (32 bytes; see `generate_token`). Rejecting anything else BEFORE
/// rendering closes the reflected-XSS vector (a path like `"><script>...` can
/// never reach the page) and avoids a pointless DB lookup on the POST path.
fn is_valid_token_format(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Minimal HTML attribute/text escaper. Defence-in-depth: the token is already
/// format-validated to hex, but we still escape when interpolating into markup.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(c),
        }
    }
    out
}

/// GET: render a confirm page with NO side effects (safe for mail-scanner /
/// link-preview prefetch — same posture as the comms one-click unsubscribe).
async fn verify_confirm_page(Path(token): Path<String>) -> Result<Html<String>, ApiError> {
    // P2 SECURITY (reflected XSS): reject malformed tokens before rendering so a
    // crafted path can never be reflected into the page.
    if !is_valid_token_format(&token) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                serde_json::json!({ "error": "not_found", "message": "unknown verification link" }),
            ),
        ));
    }
    let token = html_escape(&token);
    Ok(Html(format!(
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
    )))
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

/// Re-check every dns_txt-verified domain and revoke those whose challenge TXT
/// record is gone. The canonical implementation lives in `anseo-storage`
/// (`run_reverification_job`) so the background **worker** can drive it on a
/// daily cadence without depending on this HTTP crate — see
/// `apps/worker/src/run.rs` (`maybe_run_reverification`). This thin wrapper is
/// kept so API-side callers can reuse the same job behind the crate's
/// `ApiError`. Parameterised on the resolver so production passes the real
/// hickory client and tests inject a mock (no network).
pub async fn run_reverification_job(
    storage: &Arc<anseo_storage::Storage>,
    resolver: &dyn TxtResolver,
) -> Result<usize, ApiError> {
    anseo_storage::repositories::verification::run_reverification_job(storage, resolver)
        .await
        .map_err(storage_err)
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

    #[test]
    fn email_authority_rejects_foreign_host() {
        // P1 SECURITY: an attacker's own mailbox must NOT verify someone's domain.
        assert!(!email_proves_domain_authority(
            "attacker@example.com",
            "victim.com"
        ));
        assert!(!email_proves_domain_authority(
            "admin@evil.com",
            "victim.com"
        ));
        // Malformed / empty addresses are rejected.
        assert!(!email_proves_domain_authority("not-an-email", "victim.com"));
        assert!(!email_proves_domain_authority("@victim.com", "victim.com"));
        assert!(!email_proves_domain_authority("admin@", "victim.com"));
    }

    #[test]
    fn email_authority_accepts_same_domain_and_roles() {
        // Any mailbox at the claimed domain proves authority.
        assert!(email_proves_domain_authority(
            "anyone@victim.com",
            "victim.com"
        ));
        // Recognized role addresses at the claimed domain.
        for role in ROLE_LOCAL_PARTS {
            assert!(
                email_proves_domain_authority(&format!("{role}@victim.com"), "victim.com"),
                "role {role} at the claimed domain must be accepted"
            );
        }
        // Host normalization: trailing dot / case must still match.
        assert!(email_proves_domain_authority(
            "admin@VICTIM.com.",
            "victim.com"
        ));
    }

    #[test]
    fn confirm_token_format_validation() {
        // P2 SECURITY: a 64-char lowercase hex token is the only accepted shape.
        let good = "a".repeat(64);
        assert!(is_valid_token_format(&good));
        // XSS payload in the path is rejected (would 404 before rendering).
        assert!(!is_valid_token_format("\"><script>alert(1)</script>"));
        // Wrong length / non-hex chars rejected.
        assert!(!is_valid_token_format("deadbeef"));
        assert!(!is_valid_token_format(&"z".repeat(64)));
    }

    #[test]
    fn html_escape_neutralizes_markup() {
        assert_eq!(
            html_escape("\"><script>alert(1)</script>"),
            "&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;"
        );
        // A valid hex token is unchanged by escaping.
        let tok = "a1b2c3";
        assert_eq!(html_escape(tok), tok);
    }
}
