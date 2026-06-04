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
//!    be declared in the project (undeclared slugs get a 404, not auto-create).
//!
//! 2. **Consent + envelope gate (RISK-3).** If — and only if — the project has
//!    an *active* benchmark opt-in on the current [`TERMS_VERSION`], the
//!    contribution is routed through [`Redactor`] + envelope [sealing](ProjectKek::seal).
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

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use opengeo_benchmark::{ProjectKek, RawPromptRun, Redactor, SealedContribution, TERMS_VERSION};

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
    /// The raw response text the external provider returned. Optional when the
    /// caller has already extracted `citation_domains` itself.
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

/// Pure validation of the inbound shape. Mirrors the native write path's
/// slug-safety + non-empty checks so external runs can't smuggle in shapes the
/// redactor would later reject.
pub fn validate_request(req: &IngestRunRequest) -> Result<(), String> {
    if !is_slug_safe(&req.prompt_slug) {
        return Err(format!(
            "`prompt_slug` `{}` is not slug-safe (lowercase ASCII + digits + hyphens)",
            req.prompt_slug
        ));
    }
    if req.provider.trim().is_empty() {
        return Err("`provider` must not be empty".to_string());
    }
    if req.model.trim().is_empty() {
        return Err("`model` must not be empty".to_string());
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
    } else if let Some(text) = &req.response_text {
        opengeo_extractors::extract_citations(text)
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

async fn ingest_run(
    Extension(scope): Extension<ProjectScope>,
    State(state): State<AppState>,
    Json(req): Json<IngestRunRequest>,
) -> Result<(StatusCode, Json<IngestRunResponse>), (StatusCode, Json<serde_json::Value>)> {
    if let Err(msg) = validate_request(&req) {
        return Err(err(StatusCode::BAD_REQUEST, "validation_failed", msg));
    }

    let project_id = scope.id();

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
            StatusCode::NOT_FOUND,
            "prompt_not_found",
            format!(
                "prompt `{}` is not declared in this project — add it before ingesting external runs",
                req.prompt_slug
            ),
        ));
    };

    let observed_at = req.observed_at.unwrap_or_else(chrono::Utc::now);
    let citation_domains = resolve_citation_domains(&req);
    let run_id = opengeo_core::PromptRunId::new();
    let now = chrono::Utc::now();

    // 2. Persist the external run as a prompt_run for the resolved project.
    let raw_response = serde_json::json!({
        "kind": "external_ingest",
        "response_text": req.response_text,
        "citation_domains": citation_domains,
        "observed_rank": req.observed_rank,
    });
    let row = opengeo_storage::models::PromptRunRow {
        id: run_id,
        prompt_id: prompt.id,
        provider: req.provider.clone(),
        provider_model_version: req.model.clone(),
        provider_region: None,
        started_at: observed_at,
        finished_at: Some(observed_at),
        raw_response,
        request_parameters: serde_json::json!({ "source": "ingest_api" }),
        status: "ok".to_string(),
        error_kind: None,
        organization_id: None,
        tenant_id: None,
        created_at: now,
    };
    state
        .storage
        .prompt_runs()
        .insert(&row)
        .await
        .map_err(|e| {
            if let opengeo_storage::Error::Sqlx(sqlx::Error::Database(db_err)) = &e {
                if db_err.code().as_deref() == Some("23503") {
                    return err(
                        StatusCode::NOT_FOUND,
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

    // 3. Consent + envelope gate. Read the project's latest consent row.
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

    // Load the per-project KEK ONLY when the project actually opted in — a
    // project that never opted in needs no key and gets a clean skip. Loading
    // goes through the operator's durable secret-store chain (keyring →
    // age-file → in-memory), the same chain the rest of the API uses for
    // secrets; it can block, so run it off the async runtime.
    let project_id_str = project_id.to_string();
    let kek = if opted_in {
        match tokio::task::spawn_blocking(move || {
            let store = opengeo_core::default_chain();
            ProjectKek::load(&store, &project_id_str)
        })
        .await
        {
            Ok(Ok(kek)) => Some(kek),
            // KEK genuinely absent — handled as the explicit KekMissing status
            // below (NOT a silent drop).
            Ok(Err(_)) => None,
            Err(join_err) => {
                tracing::error!(error = %join_err, "ingest: KEK load task panicked");
                None
            }
        }
    } else {
        None
    };

    let raw = RawPromptRun {
        project_id: project_id.to_string(),
        prompt_slug: req.prompt_slug.clone(),
        provider: req.provider.clone(),
        model: req.model.clone(),
        observed_at,
        observed_rank: req.observed_rank,
        citation_domains,
        // Fields the redactor intentionally drops — never transmitted.
        brand_name: scope.name().to_string(),
        raw_response_text: req.response_text.clone().unwrap_or_default(),
        api_key_used: String::new(),
        ip_address: String::new(),
    };

    let (contribution, sealed) = decide_contribution(opted_in, &consented_terms, kek.as_ref(), raw);

    // Persisting the sealed contribution to a contributions outbox is Story
    // 40.2/40.3 (the SDK + upload path); 40.1's correctness boundary is that
    // the contribution is *produced and accounted for*, never silently
    // dropped. Log the sealed envelope's linkage id so the seal is observable.
    if let Some(sealed) = &sealed {
        tracing::info!(
            event = "ingest.contribution.sealed",
            project_hmac = %sealed.project_hmac,
            "external run sealed into a benchmark contribution"
        );
    }

    Ok((
        StatusCode::OK,
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
    use chrono::{TimeZone, Utc};
    use opengeo_benchmark::ProjectKek;
    use opengeo_core::InMemoryStore;

    const PROJECT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    fn req(text: Option<&str>, domains: Option<Vec<&str>>) -> IngestRunRequest {
        IngestRunRequest {
            prompt_slug: "vector-db".into(),
            provider: "openai".into(),
            model: "gpt-4o-2024-08-06".into(),
            response_text: text.map(str::to_string),
            citation_domains: domains.map(|d| d.into_iter().map(str::to_string).collect()),
            observed_rank: Some(2),
            observed_at: Some(Utc.with_ymd_and_hms(2026, 6, 15, 8, 43, 21).unwrap()),
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
        assert!(validate_request(&r).unwrap_err().contains("slug-safe"));
    }

    #[test]
    fn validate_rejects_empty_provider_and_model() {
        let mut r = req(None, None);
        r.provider = "".into();
        assert!(validate_request(&r).is_err());
        let mut r = req(None, None);
        r.model = "  ".into();
        assert!(validate_request(&r).is_err());
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
}
