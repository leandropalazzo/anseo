//! Story 40.1 — Run-Ingestion API (benchmark on-ramp).
//!
//! `POST /v1/ingest/run` records an **externally-executed** prompt run — a run
//! the operator (or a future SDK, Story 40.2/40.3) executed against a provider
//! outside OpenGEO's own orchestrator — and feeds it through the *same*
//! extraction → redaction → envelope-sealed-contribution path as a native run.
//!
//! Two things happen, in order:
//!
//! 1. **Persist the run.** The external run is stored as a `prompt_run` row for
//!    the resolved project via the existing [`PromptRunRepo`], exactly like the
//!    native [`crate::routes::prompt_runs`] write path. The prompt must already
//!    be declared in the project (undeclared slugs get a `422 prompt_not_found`,
//!    not auto-create); an unresolvable provider gets `422 provider_not_supported`.
//!    A well-formed request returns `202 { run_id, … }`.
//!
//! 2. **Per-run `contribute` flag + KEK hard gate (AC-3, RISK-3).** The request
//!    carries `contribute: bool` (default `false`; ships in the schema from
//!    Story 40.1 so the SDK clients never need a breaking update). A request
//!    with `contribute: true` but no per-project KEK (Story 39.1) is rejected
//!    up-front with `403 kek_missing` — the run is not recorded under a false
//!    promise of contribution. `contribute: false` proceeds regardless of KEK
//!    state.
//!
//!    Beyond the gate, a run is sealed only when it BOTH set `contribute: true`
//!    AND the project has an *active* benchmark opt-in on the current
//!    [`TERMS_VERSION`]; the contribution is then routed through [`Redactor`] +
//!    envelope [sealing](ProjectKek::seal).
//!    Sealing REQUIRES a per-project KEK (Story 39.1). The critical correctness
//!    rule: **benchmark data is never silently dropped.** If the project opted
//!    in but no KEK can be loaded, we do NOT skip quietly — the run is persisted
//!    and the response carries an explicit `contribution.status = "kek_missing"`
//!    (HTTP 200, the run *was* recorded) so the caller learns the contribution
//!    did not seal and can provision a KEK. A project that never opted in gets
//!    `contribution.status = "skipped_not_opted_in"`; a successful seal reports
//!    `"sealed"`.
//!
//! The redaction guarantee is identical to native runs: a `BenchmarkPayload`
//! can only be produced by [`Redactor::redact`] (private fields, no public
//! constructor) and can only be sealed by [`ProjectKek::seal`] (no KEK ⇒ no
//! contribution). This module never hand-builds either.
//!
//! # Compile-time parity invariant (Story 40.4, AC-4)
//!
//! The ingest path has the SAME compile-time redaction guarantee as a native
//! run: there is no way to manufacture a benchmark contribution without first
//! passing a [`RawPromptRun`] through [`Redactor::redact`] and then through
//! [`ProjectKek::seal`]. A would-be ingest job cannot hand-roll a
//! [`BenchmarkPayload`] (no public constructor / no public fields):
//!
//! ```compile_fail
//! use anseo_benchmark::BenchmarkPayload;
//! // The ingest path could only contribute by building one of these — but the
//! // struct has private fields and no constructor, so this never compiles. The
//! // only door is `Redactor::redact`, which requires a loaded `ProjectKek`.
//! let _payload = BenchmarkPayload {
//!     prompt_slug: "vector-db".into(),
//!     provider: "openai".into(),
//! };
//! ```

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anseo_benchmark::{ProjectKek, RawPromptRun, Redactor, SealedContribution, TERMS_VERSION};
use anseo_core::ids::CitationId;
use anseo_extractors::SourceType;
use anseo_storage::models::CitationRow;
use anseo_storage::repositories::anonymous_contributions::AnonymousContributionToStore;
use anseo_storage::repositories::prompt_runs::PromptRunRepo;

use crate::extractors::project::ProjectScope;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/ingest/run", post(ingest_run))
}

/// One externally-executed run, as submitted by the caller.
#[derive(Debug, Clone, Deserialize)]
pub struct IngestRunRequest {
    /// Declared prompt slug within the resolved project. Must already exist.
    pub prompt_slug: String,
    pub provider: String,
    pub model: String,
    /// Canonical external provider payload. May be plain text or structured
    /// JSON, depending on what the caller captured.
    #[serde(default)]
    pub raw_response: Option<serde_json::Value>,
    /// Caller-supplied run metadata. Persisted under
    /// `request_parameters.metadata`.
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    /// Compatibility field for early clients that only captured plain response
    /// text before `raw_response` became canonical.
    #[serde(default)]
    pub response_text: Option<String>,
    /// Source domains observed in the run's citations. When omitted and
    /// `response_text` is present, the domains are extracted from the text.
    #[serde(default)]
    pub citation_domains: Option<Vec<String>>,
    /// The brand's observed rank in this run, if the caller computed it.
    #[serde(default)]
    pub observed_rank: Option<i32>,
    /// When the external run was observed. Defaults to now if omitted.
    #[serde(default)]
    pub observed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Opt this specific run into the anonymous benchmark contribution path.
    ///
    /// Defaults to `false`. Ships in the schema from day one (Story 40.1) so
    /// the SDK clients (40.2/40.3) don't need a breaking update. As of Story
    /// 40.4 the flag is fully enforced: a `true` request with no per-project KEK
    /// is rejected `403 kek_missing` (hard gate), and the run is redacted +
    /// envelope-sealed only when it set `contribute: true` AND the project has
    /// an active benchmark opt-in on the current terms (the narrower-of-two
    /// gates). `false` (the default) is recorded/extracted locally only.
    #[serde(default)]
    pub contribute: bool,
}

/// Why the benchmark contribution leg did (or did not) produce a sealed
/// contribution. Serialized into the response so the caller is never left
/// guessing whether benchmark data was contributed, skipped, or blocked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ContributionStatus {
    /// Project opted in on the current terms AND a KEK was available: the
    /// run was redacted and sealed.
    Sealed,
    /// Project has no active opt-in on the current terms — nothing to seal.
    SkippedNotOptedIn,
    /// HARD GATE: project opted in but no per-project KEK could be loaded, so
    /// the contribution could NOT be sealed. The run is still persisted; the
    /// benchmark data is explicitly flagged here rather than silently dropped.
    KekMissing,
    /// Redaction refused the run (stale consent or an invalid slug). The run is
    /// persisted; the contribution is reported as blocked with the reason.
    RedactionRejected { reason: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestRunResponse {
    pub run_id: String,
    pub project_id: String,
    pub prompt_slug: String,
    pub provider: String,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub contribution: ContributionStatus,
}

const INGEST_RATE_LIMIT_MAX: usize = 60;
const INGEST_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
struct ProjectRateLimiter {
    windows: Mutex<HashMap<String, Vec<Instant>>>,
}

impl ProjectRateLimiter {
    fn check(&self, project_id: &str, now: Instant) -> bool {
        let mut windows = self.windows.lock().expect("ingest rate limiter poisoned");
        let window = windows.entry(project_id.to_string()).or_default();
        window.retain(|seen| now.duration_since(*seen) < INGEST_RATE_LIMIT_WINDOW);
        if window.len() >= INGEST_RATE_LIMIT_MAX {
            return false;
        }
        window.push(now);
        true
    }
}

fn ingest_rate_limiter() -> &'static ProjectRateLimiter {
    static LIMITER: OnceLock<ProjectRateLimiter> = OnceLock::new();
    LIMITER.get_or_init(ProjectRateLimiter::default)
}

/// Why a request was rejected at the pure-validation stage. Distinguished so
/// the handler can map each to the AC-mandated status code:
/// `provider_not_supported` is a `422` (the body is well-formed but names a
/// provider that doesn't resolve), while shape problems are a `400`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// The body is malformed (bad slug, empty model). Maps to `400`.
    BadRequest(String),
    /// `provider` does not resolve to a first-party name or `plugin:<id>`.
    /// Maps to `422 provider_not_supported`.
    ProviderNotSupported(String),
}

/// Pure validation of the inbound shape. Mirrors the native write path's
/// slug-safety + non-empty checks so external runs can't smuggle in shapes the
/// redactor would later reject, and resolves the provider against the same
/// [`anseo_core::ProviderName`] grammar the orchestrator uses (first-party
/// names OR `plugin:<id>`).
pub fn validate_request(req: &IngestRunRequest) -> Result<(), ValidationError> {
    if !is_slug_safe(&req.prompt_slug) {
        return Err(ValidationError::BadRequest(format!(
            "`prompt_slug` `{}` is not slug-safe (lowercase ASCII + digits + hyphens)",
            req.prompt_slug
        )));
    }
    if req.model.trim().is_empty() {
        return Err(ValidationError::BadRequest(
            "`model` must not be empty".to_string(),
        ));
    }
    if req.raw_response.is_none() && req.response_text.is_none() {
        return Err(ValidationError::BadRequest(
            "either `raw_response` or compatibility field `response_text` must be supplied"
                .to_string(),
        ));
    }
    if req
        .metadata
        .as_ref()
        .is_some_and(|metadata| !metadata.is_object())
    {
        return Err(ValidationError::BadRequest(
            "`metadata` must be a JSON object when supplied".to_string(),
        ));
    }
    if anseo_core::ProviderName::parse(req.provider.trim()).is_none() {
        return Err(ValidationError::ProviderNotSupported(format!(
            "provider `{}` is not supported (expected a first-party name or `plugin:<id>`)",
            req.provider
        )));
    }
    Ok(())
}

fn is_slug_safe(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether a project's most-recent consent row is an *active* opt-in on the
/// current terms. Pure so it is unit-tested without a DB.
pub fn consent_is_active(event: &str, terms_version: &str) -> bool {
    event == "optin" && terms_version == TERMS_VERSION
}

/// Decide and (when applicable) produce the benchmark contribution for an
/// already-persisted external run. Pure: takes the consent decision and an
/// optional KEK, returns the status (+ the sealed contribution to persist).
///
/// This is the gate-critical core, deliberately free of HTTP/DB so the
/// no-silent-drop guarantee is exercised by always-run unit tests:
///
/// - not opted in → [`ContributionStatus::SkippedNotOptedIn`], no payload.
/// - opted in, KEK present → redact + seal → [`ContributionStatus::Sealed`].
/// - opted in, KEK absent → [`ContributionStatus::KekMissing`] (NEVER a silent
///   skip): the caller must surface this so the operator provisions a KEK.
/// - redaction refuses (stale terms / bad slug) → [`ContributionStatus::RedactionRejected`].
pub fn decide_contribution(
    opted_in: bool,
    consented_terms: &str,
    kek: Option<&ProjectKek>,
    raw: RawPromptRun,
) -> (ContributionStatus, Option<SealedContribution>) {
    if !opted_in {
        return (ContributionStatus::SkippedNotOptedIn, None);
    }
    let Some(kek) = kek else {
        // HARD GATE — the project wants to contribute but has no KEK. Do NOT
        // silently drop: flag it loudly so the operator can provision one.
        return (ContributionStatus::KekMissing, None);
    };
    let redactor = Redactor::new(kek, consented_terms);
    match redactor.redact(raw) {
        Ok(payload) => match kek.seal(&payload) {
            Ok(sealed) => (ContributionStatus::Sealed, Some(sealed)),
            // Sealing failure is a cryptographic/serialization fault, not a
            // silent drop — report it as a redaction-class rejection.
            Err(e) => (
                ContributionStatus::RedactionRejected {
                    reason: format!("seal failed: {e}"),
                },
                None,
            ),
        },
        Err(e) => (
            ContributionStatus::RedactionRejected {
                reason: e.to_string(),
            },
            None,
        ),
    }
}

/// Resolve the citation domains for the run: explicit `citation_domains` win;
/// otherwise extract from `response_text`; otherwise empty. Deduplicated and
/// lowercased for stability. Pure.
pub fn resolve_citation_domains(req: &IngestRunRequest) -> Vec<String> {
    let mut domains: Vec<String> = if let Some(explicit) = &req.citation_domains {
        explicit.clone()
    } else if let Some(text) = message_text_for_extraction(req) {
        anseo_extractors::extract_citations(&text)
            .into_iter()
            .map(|c| c.domain)
            .collect()
    } else {
        Vec::new()
    };
    for d in domains.iter_mut() {
        *d = d.trim().to_ascii_lowercase();
    }
    domains.retain(|d| !d.is_empty());
    domains.sort();
    domains.dedup();
    domains
}

fn err(status: StatusCode, error: &str, message: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

fn sealed_payload_json(
    sealed: &SealedContribution,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    serde_json::to_value(sealed).map_err(|e| {
        tracing::error!(error = %e, "ingest: failed to serialize sealed contribution");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "persist_failed",
            "failed to serialize sealed contribution".to_string(),
        )
    })
}

fn persisted_raw_response(
    req: &IngestRunRequest,
    citation_domains: &[String],
) -> serde_json::Value {
    if let Some(raw) = &req.raw_response {
        return raw.clone();
    }
    serde_json::json!({
        "kind": "external_ingest_compat",
        "response_text": req.response_text,
        "citation_domains": citation_domains,
        "observed_rank": req.observed_rank,
    })
}

fn message_text_for_extraction(req: &IngestRunRequest) -> Option<String> {
    if let Some(text) = req
        .response_text
        .as_ref()
        .filter(|text| !text.trim().is_empty())
    {
        return Some(text.clone());
    }
    let raw = req.raw_response.as_ref()?;
    let mut fragments = Vec::new();
    collect_text_fragments(raw, &mut fragments);
    if fragments.is_empty() {
        None
    } else {
        Some(fragments.join("\n"))
    }
}

fn collect_text_fragments(value: &serde_json::Value, fragments: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                fragments.push(trimmed.to_string());
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_text_fragments(item, fragments);
            }
        }
        serde_json::Value::Object(map) => {
            for key in [
                "text",
                "output_text",
                "response_text",
                "content",
                "message",
                "choices",
                "output",
                "messages",
            ] {
                if let Some(value) = map.get(key) {
                    collect_text_fragments(value, fragments);
                }
            }
        }
        _ => {}
    }
}

fn has_annotation_citations(raw_response: &serde_json::Value) -> bool {
    !anseo_extractors::extract_citations_from_annotations(raw_response).is_empty()
}

async fn persist_explicit_citation_domains(
    state: &AppState,
    run_id: anseo_core::PromptRunId,
    now: chrono::DateTime<chrono::Utc>,
    domains: &[String],
) -> Result<(), anseo_storage::Error> {
    let mut seen = HashSet::new();
    for domain in domains {
        let normalized = domain.trim().to_lowercase();
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        state
            .storage
            .citations()
            .insert(&CitationRow {
                id: CitationId::new(),
                prompt_run_id: run_id,
                url: None,
                domain: normalized.clone(),
                frequency: 1,
                source_type: Some(SourceType::GeneralWeb.as_wire_str().to_string()),
                organization_id: None,
                tenant_id: None,
                created_at: now,
            })
            .await?;
    }
    Ok(())
}

async fn ingest_run(
    Extension(scope): Extension<ProjectScope>,
    State(state): State<AppState>,
    Json(req): Json<IngestRunRequest>,
) -> Result<(StatusCode, Json<IngestRunResponse>), (StatusCode, Json<serde_json::Value>)> {
    match validate_request(&req) {
        Ok(()) => {}
        Err(ValidationError::BadRequest(msg)) => {
            return Err(err(StatusCode::BAD_REQUEST, "validation_failed", msg));
        }
        Err(ValidationError::ProviderNotSupported(msg)) => {
            return Err(err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "provider_not_supported",
                msg,
            ));
        }
    }

    let project_id = scope.id();
    if !ingest_rate_limiter().check(&project_id.to_string(), Instant::now()) {
        return Err(err(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            format!(
                "project rate limit exceeded ({} requests per {}s window)",
                INGEST_RATE_LIMIT_MAX,
                INGEST_RATE_LIMIT_WINDOW.as_secs()
            ),
        ));
    }

    // 1. The prompt must be declared in THIS project (scoping boundary). An
    //    undeclared slug is a 404 with a pointer, never an auto-create.
    let prompt = state
        .storage
        .prompts()
        .find_by_name(project_id, &req.prompt_slug)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "ingest: prompt lookup failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "failed to look up prompt".to_string(),
            )
        })?;
    let Some(prompt) = prompt else {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "prompt_not_found",
            format!(
                "prompt `{}` is not declared in this project — add it before ingesting external runs",
                req.prompt_slug
            ),
        ));
    };

    // 1b. KEK hard gate (AC-3, RISK-3). A caller that asks to `contribute` is
    //     asserting "seal this into the benchmark"; sealing is impossible
    //     without a per-project KEK (Story 39.1). Reject up-front with
    //     `403 kek_missing` so the run is NOT recorded under a false promise of
    //     contribution. `contribute: false` (the default) skips this entirely
    //     and proceeds regardless of KEK state. Load once and reuse below so the
    //     contribution leg doesn't re-load.
    //
    //     As of Story 40.4 the full consent/redaction enforcement is wired: past
    //     this gate the run is redacted + sealed iff it BOTH set `contribute:
    //     true` AND the project carries an active benchmark opt-in (below).
    let project_id_str = project_id.to_string();
    let kek = if req.contribute {
        let pid = project_id_str.clone();
        match tokio::task::spawn_blocking(move || {
            let store = anseo_core::default_chain();
            ProjectKek::load(&store, &pid)
        })
        .await
        {
            Ok(Ok(kek)) => Some(kek),
            Ok(Err(_)) => {
                return Err(err(
                    StatusCode::FORBIDDEN,
                    "kek_missing",
                    "this run requested `contribute: true` but the project has no per-project \
                     benchmark KEK — provision one before contributing external runs"
                        .to_string(),
                ));
            }
            Err(join_err) => {
                tracing::error!(error = %join_err, "ingest: KEK load task panicked");
                return Err(err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "failed to load project benchmark key".to_string(),
                ));
            }
        }
    } else {
        None
    };

    let observed_at = req.observed_at.unwrap_or_else(chrono::Utc::now);
    let citation_domains = resolve_citation_domains(&req);
    let run_id = anseo_core::PromptRunId::new();
    let now = chrono::Utc::now();
    let raw_response = persisted_raw_response(&req, &citation_domains);
    let message_text = message_text_for_extraction(&req);
    let request_metadata = req
        .metadata
        .clone()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

    // 2. Persist the external run as a prompt_run for the resolved project.
    let row = anseo_storage::models::PromptRunRow {
        id: run_id,
        prompt_id: prompt.id,
        provider: req.provider.clone(),
        provider_model_version: req.model.clone(),
        provider_region: None,
        started_at: observed_at,
        finished_at: Some(observed_at),
        raw_response: raw_response.clone(),
        request_parameters: serde_json::json!({
            "source": "ingest_api",
            "metadata": request_metadata,
            "observed_rank": req.observed_rank,
        }),
        status: "ok".to_string(),
        error_kind: None,
        organization_id: None,
        tenant_id: None,
        created_at: now,
    };
    // 3. Consent + envelope gate. Read the project's latest consent row before
    //    we open the write transaction so the decision is made once per
    //    request, then persisted atomically with the prompt_run.
    let consent = state
        .storage
        .benchmark_consent()
        .latest_for_project(project_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "ingest: consent lookup failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "failed to read benchmark consent".to_string(),
            )
        })?;

    let opted_in = consent
        .as_ref()
        .map(|c| consent_is_active(&c.event, &c.terms_version))
        .unwrap_or(false);
    let consented_terms = consent
        .as_ref()
        .map(|c| c.terms_version.clone())
        .unwrap_or_default();

    // The KEK was already loaded by the `contribute` gate above (and the
    // request was rejected `403 kek_missing` if absent), so no second secret-
    // store round-trip here. A `contribute: false` request never loaded a KEK
    // and `kek` is `None`, which `decide_contribution` reports as a clean skip.
    let raw = RawPromptRun {
        project_id: project_id.to_string(),
        prompt_slug: req.prompt_slug.clone(),
        provider: req.provider.clone(),
        model: req.model.clone(),
        observed_at,
        observed_rank: req.observed_rank,
        citation_domains: citation_domains.clone(),
        // Fields the redactor intentionally drops — never transmitted.
        brand_name: scope.name().to_string(),
        raw_response_text: message_text.clone().unwrap_or_default(),
        api_key_used: String::new(),
        ip_address: String::new(),
    };

    // A run only enters the contribution path when it BOTH opted into the
    // benchmark (durable project consent) AND set `contribute: true` on this
    // request. `contribute: false` short-circuits to a clean skip regardless of
    // project consent — the per-run flag is the narrower of the two gates.
    let contribute_this_run = req.contribute && opted_in;
    let (contribution, sealed) =
        decide_contribution(contribute_this_run, &consented_terms, kek.as_ref(), raw);

    let mut tx = state.storage.pool().begin().await.map_err(|e| {
        tracing::error!(error = %e, "ingest: failed to begin transaction");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "persist_failed",
            "failed to persist ingested run".to_string(),
        )
    })?;
    PromptRunRepo::new(state.storage.pool())
        .insert_in_tx(&mut tx, &row)
        .await
        .map_err(|e| {
            if let anseo_storage::Error::Sqlx(sqlx::Error::Database(db_err)) = &e {
                if db_err.code().as_deref() == Some("23503") {
                    return err(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "prompt_not_found",
                        "prompt was deleted between lookup and insert; retry will re-validate"
                            .to_string(),
                    );
                }
            }
            tracing::error!(error = %e, "ingest: prompt run insert failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "persist_failed",
                "failed to persist ingested run".to_string(),
            )
        })?;

    // Persist the sealed anonymous contribution durably so ingest follows the
    // same redaction + crypto-shred path as native runs.
    if let Some(sealed) = &sealed {
        let consent_record_id = consent.as_ref().map(|row| row.id).ok_or_else(|| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "sealed contribution had no authorizing consent row".to_string(),
            )
        })?;
        let sealed_payload = sealed_payload_json(sealed)?;
        state
            .storage
            .anonymous_contributions()
            .insert_in_tx(
                &mut tx,
                &AnonymousContributionToStore {
                    prompt_run_id: run_id,
                    project_id,
                    project_hmac: sealed.project_hmac.clone(),
                    consent_record_id,
                    terms_version: consented_terms.clone(),
                    sealed_payload,
                },
            )
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "ingest: anonymous contribution insert failed");
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "persist_failed",
                    "failed to persist sealed benchmark contribution".to_string(),
                )
            })?;
        tracing::info!(
            event = "ingest.contribution.sealed",
            project_hmac = %sealed.project_hmac,
            "external run sealed into a benchmark contribution"
        );
    }

    tx.commit().await.map_err(|e| {
        tracing::error!(error = %e, "ingest: transaction commit failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "persist_failed",
            "failed to persist ingested run".to_string(),
        )
    })?;

    let should_extract = message_text.is_some() || has_annotation_citations(&raw_response);
    if let (Some(config), true) = (state.config.as_ref(), should_extract) {
        if let Err(e) = anseo_extractors::extract_and_persist(
            &state.storage,
            config,
            run_id,
            message_text.as_deref().unwrap_or(""),
            &raw_response,
            now,
        )
        .await
        {
            tracing::warn!(error = %e, "ingest: mention/citation extraction failed");
        }
    }
    if message_text.is_none()
        && !has_annotation_citations(&raw_response)
        && req.citation_domains.is_some()
    {
        if let Err(e) =
            persist_explicit_citation_domains(&state, run_id, now, &citation_domains).await
        {
            tracing::warn!(error = %e, "ingest: explicit citation-domain persistence failed");
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(IngestRunResponse {
            run_id: run_id.to_string(),
            project_id: project_id.to_string(),
            prompt_slug: req.prompt_slug,
            provider: req.provider,
            observed_at,
            contribution,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_benchmark::ProjectKek;
    use anseo_core::InMemoryStore;
    use chrono::{TimeZone, Utc};

    const PROJECT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    fn req(text: Option<&str>, domains: Option<Vec<&str>>) -> IngestRunRequest {
        IngestRunRequest {
            prompt_slug: "vector-db".into(),
            provider: "openai".into(),
            model: "gpt-4o-2024-08-06".into(),
            raw_response: text.map(|text| serde_json::Value::String(text.to_string())),
            metadata: None,
            response_text: text.map(str::to_string),
            citation_domains: domains.map(|d| d.into_iter().map(str::to_string).collect()),
            observed_rank: Some(2),
            observed_at: Some(Utc.with_ymd_and_hms(2026, 6, 15, 8, 43, 21).unwrap()),
            contribute: false,
        }
    }

    fn raw() -> RawPromptRun {
        RawPromptRun {
            project_id: PROJECT.into(),
            prompt_slug: "vector-db".into(),
            provider: "openai".into(),
            model: "gpt-4o-2024-08-06".into(),
            observed_at: Utc.with_ymd_and_hms(2026, 6, 15, 8, 43, 21).unwrap(),
            observed_rank: Some(2),
            citation_domains: vec!["docs.example.com".into()],
            brand_name: "Pinecone".into(),
            raw_response_text: "Pinecone is a leading vector database…".into(),
            api_key_used: String::new(),
            ip_address: String::new(),
        }
    }

    fn test_kek() -> ProjectKek {
        let store = InMemoryStore::durable_for_tests();
        ProjectKek::load_or_create(&store, PROJECT).unwrap()
    }

    #[test]
    fn validate_rejects_non_slug_prompt() {
        let mut r = req(None, None);
        r.prompt_slug = "Vector DB".into();
        assert!(matches!(
            validate_request(&r),
            Err(ValidationError::BadRequest(m)) if m.contains("slug-safe")
        ));
    }

    #[test]
    fn validate_rejects_empty_or_unknown_provider() {
        // Empty and unknown providers both fail to resolve → ProviderNotSupported.
        let mut r = req(Some("hello"), None);
        r.provider = "".into();
        assert!(matches!(
            validate_request(&r),
            Err(ValidationError::ProviderNotSupported(_))
        ));
        let mut r = req(Some("hello"), None);
        r.provider = "totally-made-up".into();
        assert!(matches!(
            validate_request(&r),
            Err(ValidationError::ProviderNotSupported(_))
        ));
    }

    #[test]
    fn validate_rejects_empty_model_as_bad_request() {
        let mut r = req(None, None);
        r.model = "  ".into();
        assert!(matches!(
            validate_request(&r),
            Err(ValidationError::BadRequest(_))
        ));
    }

    #[test]
    fn validate_requires_raw_response_or_compat_response_text() {
        let mut r = req(None, None);
        r.raw_response = None;
        r.response_text = None;
        assert!(matches!(
            validate_request(&r),
            Err(ValidationError::BadRequest(m)) if m.contains("raw_response")
        ));
    }

    #[test]
    fn validate_rejects_non_object_metadata() {
        let mut r = req(Some("hello"), None);
        r.metadata = Some(serde_json::json!("trace-123"));
        assert!(matches!(
            validate_request(&r),
            Err(ValidationError::BadRequest(m)) if m.contains("`metadata`")
        ));
    }

    #[test]
    fn validate_accepts_first_party_and_plugin_providers() {
        let mut r = req(Some("hello"), None);
        r.provider = "anthropic".into();
        assert!(validate_request(&r).is_ok());
        r.provider = "plugin:test.mock-provider".into();
        assert!(validate_request(&r).is_ok());
    }

    #[test]
    fn contribute_defaults_to_false_when_omitted() {
        let raw = r#"{"prompt_slug":"vector-db","provider":"openai","model":"gpt-4o"}"#;
        let r: IngestRunRequest = serde_json::from_str(raw).unwrap();
        assert!(!r.contribute);
    }

    #[test]
    fn consent_active_only_for_optin_on_current_terms() {
        assert!(consent_is_active("optin", TERMS_VERSION));
        assert!(!consent_is_active("optout", TERMS_VERSION));
        assert!(!consent_is_active("optin", "v0-stale"));
    }

    #[test]
    fn citation_domains_explicit_take_precedence() {
        let r = req(
            Some("see https://docs.foo.com/x"),
            Some(vec!["Bar.COM", "bar.com"]),
        );
        // Explicit list wins, lowercased + deduped; response text ignored.
        assert_eq!(resolve_citation_domains(&r), vec!["bar.com".to_string()]);
    }

    #[test]
    fn citation_domains_extracted_from_text_when_absent() {
        let r = req(
            Some("read https://docs.example.com/guide and example.org"),
            None,
        );
        let domains = resolve_citation_domains(&r);
        assert!(domains.contains(&"docs.example.com".to_string()));
    }

    #[test]
    fn message_text_prefers_compat_response_text() {
        let r = req(Some("compat text"), None);
        assert_eq!(
            message_text_for_extraction(&r),
            Some("compat text".to_string())
        );
    }

    #[test]
    fn message_text_uses_canonical_raw_response_when_needed() {
        let mut r = req(None, None);
        r.raw_response = Some(serde_json::json!({"text": "hello"}));
        assert_eq!(message_text_for_extraction(&r), Some("hello".to_string()));

        r.raw_response = Some(serde_json::json!("plain string"));
        assert_eq!(
            message_text_for_extraction(&r),
            Some("plain string".to_string())
        );
    }

    #[test]
    fn message_text_uses_nested_provider_payloads() {
        let mut r = req(None, None);
        r.raw_response = Some(serde_json::json!({
            "choices": [{
                "message": {
                    "content": [
                        { "text": "first" },
                        { "text": "second" }
                    ]
                }
            }]
        }));
        assert_eq!(
            message_text_for_extraction(&r),
            Some("first\nsecond".to_string())
        );
    }

    #[test]
    fn persisted_raw_response_preserves_supplied_json() {
        let mut r = req(None, None);
        r.raw_response = Some(serde_json::json!({"id": "abc", "text": "hello"}));
        let persisted = persisted_raw_response(&r, &[]);
        assert_eq!(persisted["id"], "abc");
        assert_eq!(persisted["text"], "hello");
    }

    #[test]
    fn limiter_blocks_after_capacity_then_recovers_after_window() {
        let limiter = ProjectRateLimiter::default();
        let base = Instant::now();
        for _ in 0..INGEST_RATE_LIMIT_MAX {
            assert!(limiter.check(PROJECT, base));
        }
        assert!(!limiter.check(PROJECT, base));
        assert!(limiter.check(
            PROJECT,
            base + INGEST_RATE_LIMIT_WINDOW + Duration::from_millis(1)
        ));
    }

    #[test]
    fn not_opted_in_skips_cleanly() {
        let (status, sealed) = decide_contribution(false, "", None, raw());
        assert_eq!(status, ContributionStatus::SkippedNotOptedIn);
        assert!(sealed.is_none());
    }

    #[test]
    fn opted_in_with_kek_seals() {
        let kek = test_kek();
        let (status, sealed) = decide_contribution(true, TERMS_VERSION, Some(&kek), raw());
        assert_eq!(status, ContributionStatus::Sealed);
        let sealed = sealed.expect("a sealed contribution");
        // Round-trips back to the redacted payload under the same KEK.
        let opened = kek.open(&sealed).unwrap();
        assert_eq!(opened.prompt_slug(), "vector-db");
        // Confidential fields never reach the sealed wire form.
        let wire = serde_json::to_string(&sealed).unwrap();
        assert!(!wire.contains("Pinecone"));
        assert!(!wire.contains("vector database"));
    }

    #[test]
    fn opted_in_without_kek_is_flagged_not_dropped() {
        // THE gate-critical case: opted in, no KEK ⇒ explicit KekMissing,
        // never a silent skip and never a sealed contribution.
        let (status, sealed) = decide_contribution(true, TERMS_VERSION, None, raw());
        assert_eq!(status, ContributionStatus::KekMissing);
        assert!(sealed.is_none());
    }

    #[test]
    fn opted_in_with_stale_terms_is_rejected_not_sealed() {
        let kek = test_kek();
        let (status, sealed) = decide_contribution(true, "v0-stale", Some(&kek), raw());
        assert!(matches!(
            status,
            ContributionStatus::RedactionRejected { .. }
        ));
        assert!(sealed.is_none());
    }

    #[test]
    fn kek_missing_status_serializes_explicitly() {
        let v = serde_json::to_value(ContributionStatus::KekMissing).unwrap();
        assert_eq!(v["status"], "kek_missing");
    }

    /// Story 40.4 AC-2 / "narrower-of-two-gates": the per-run `contribute` flag
    /// is ANDed with the project's durable opt-in. `contribute: false` short-
    /// circuits to a clean skip even when the project IS opted in and a KEK is
    /// present — the run is recorded, but no contribution is sealed. This mirrors
    /// the handler's `contribute_this_run = req.contribute && opted_in` gate.
    #[test]
    fn contribute_false_never_seals_even_when_opted_in_with_kek() {
        let kek = test_kek();
        let req_contribute = false;
        let opted_in = true;
        let contribute_this_run = req_contribute && opted_in;
        let (status, sealed) =
            decide_contribution(contribute_this_run, TERMS_VERSION, Some(&kek), raw());
        assert_eq!(status, ContributionStatus::SkippedNotOptedIn);
        assert!(
            sealed.is_none(),
            "contribute=false must NEVER produce a sealed contribution"
        );
    }

    /// Story 40.4 AC-1 redaction parity: a contribution sealed from the ingest
    /// path is byte-for-byte the SAME projection a native run would produce
    /// through the identical `Redactor` — confidential fields (brand, raw text,
    /// secrets, IP) are dropped and the timestamp is k-anonymized to the hour.
    /// This pins that ingest reuses, rather than re-implements, the redactor.
    #[test]
    fn ingest_seal_matches_native_redactor_projection() {
        let kek = test_kek();
        // The native redaction of the same RawPromptRun.
        let native = Redactor::new(&kek, TERMS_VERSION)
            .redact(raw())
            .expect("native redact");
        // The ingest contribution path.
        let (status, sealed) = decide_contribution(true, TERMS_VERSION, Some(&kek), raw());
        assert_eq!(status, ContributionStatus::Sealed);
        let opened = kek.open(&sealed.expect("sealed")).unwrap();
        // Same narrow public projection (no brand_name / raw text accessor even
        // exists), incl. the hour-rounded observation time.
        assert_eq!(opened.prompt_slug(), native.prompt_slug());
        assert_eq!(opened.provider(), native.provider());
        assert_eq!(opened.model(), native.model());
        assert_eq!(opened.observed_at_hour(), native.observed_at_hour());
        assert_eq!(opened.citation_domains(), native.citation_domains());
        assert_eq!(opened.terms_version(), native.terms_version());
    }
}
