//! Identified-contribution ingest — Epic 44 / Story 44.2.
//!
//! `POST /v1/benchmark/contributions` — the SERVER side of the identified tier.
//! It consumes exactly what the OSS client (44.1) produces: a
//! [`SealedContribution`] whose AEAD AAD binds a **verification token** (43.2),
//! never a raw brand name. The flow, in order:
//!
//!   1. **Authenticate the request.** HMAC / API-key auth is enforced upstream
//!      by the gated `v1_routes` auth layer — a bad signature never reaches this
//!      handler (it is rejected with 401). This handler therefore starts from an
//!      authenticated [`ProjectScope`].
//!   2. **Load the per-project KEK (HARD GATE, 39.1).** No KEK ⇒ the identity
//!      write is refused (403, `kek_missing`) — identified data is never written
//!      without the key that can later crypto-shred it (G-SHRED).
//!   3. **Open + authenticate the sealed payload.** [`ProjectKek::open`] both
//!      decrypts and authenticates: the AAD binds `project_hmac` + the
//!      verification token, so a tampered or swapped token fails here. Failure ⇒
//!      403 (`sealed_payload_rejected`) — distinct from the 401 HMAC class
//!      (AC-2).
//!   4. **Resolve the token → verified brand SERVER-SIDE.** The token is hashed
//!      and matched in the entity registry; it resolves ONLY to a domain whose
//!      entity is currently `claim_status = 'verified'` (AC-3). Anything else
//!      (unverified / unknown token) is refused with 403.
//!   5. **Persist** the contribution attributed to that brand via the registry
//!      FK (`contributions.entity_domain`) — the raw domain is never a cleartext
//!      body field (AC-1) — gated on the consent provenance FK (CC-NFR2).
//!   6. **Audit every resolution** — accepted and refused alike (CC-NFR2).
//!
//! Geo-gating (44.4) is applied by `geo_gate_middleware`, which already lists
//! `/v1/benchmark/contributions` as an identified-tier prefix and rejects
//! high-friction jurisdictions with 403 before the body is read.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use anseo_benchmark::{ProjectKek, SealedContribution};
use anseo_storage::repositories::contributions::{
    IdentifiedContribution, ResolutionAudit, ResolutionDecision, ResolutionOutcome,
};

use crate::extractors::project::ProjectScope;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/benchmark/contributions", post(ingest_contribution))
}

/// Inbound identified contribution. The brand is NOT named anywhere — identity
/// rides only as the verification token bound into `sealed`'s AAD.
#[derive(Debug, Clone, Deserialize)]
pub struct ContributionRequest {
    /// The envelope-sealed, redacted contribution. Carries `project_hmac` and
    /// the (cleartext-on-the-wire but AAD-bound) `verification_token`.
    pub sealed: SealedContribution,
    /// Consent event (brand_visibility tier) that authorized this identified
    /// contribution (CC-NFR2 provenance FK).
    pub consent_record_id: uuid::Uuid,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContributionResponse {
    pub contribution_id: String,
    pub entity_domain: String,
    pub project_id: String,
}

fn err(status: StatusCode, error: &str, message: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

async fn ingest_contribution(
    Extension(scope): Extension<ProjectScope>,
    State(state): State<AppState>,
    Json(req): Json<ContributionRequest>,
) -> Result<(StatusCode, Json<ContributionResponse>), (StatusCode, Json<serde_json::Value>)> {
    let project_id = scope.id();
    let project_id_str = project_id.to_string();

    // The verification token is required on the identified path: an anonymous
    // seal (no token) has no brand to resolve and does not belong here.
    let Some(verification_token) = req.sealed.verification_token.clone() else {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "missing_verification_token",
            "identified contributions must carry a verification_token; \
             anonymous-tier seals belong on the aggregate path"
                .to_string(),
        ));
    };

    let contributions = state.storage.contributions();

    // 1. HARD GATE (39.1): load the per-project KEK. No KEK ⇒ no identity write.
    //    Loading walks the operator's durable secret-store chain (can block), so
    //    run it off the async runtime.
    let pid_for_load = project_id_str.clone();
    let kek = match tokio::task::spawn_blocking(move || {
        let store = anseo_core::default_chain();
        ProjectKek::load(&store, &pid_for_load)
    })
    .await
    {
        Ok(Ok(kek)) => kek,
        Ok(Err(_)) => {
            // Audit the refusal, then 403 — never write identity without a KEK.
            let _ = contributions
                .audit_resolution(&ResolutionAudit {
                    project_id,
                    verification_token: &verification_token,
                    resolved_domain: None,
                    claim_status: None,
                    decision: ResolutionDecision::KekMissing,
                    reason: Some("no per-project KEK available for identified write"),
                    contribution_id: None,
                })
                .await;
            return Err(err(
                StatusCode::FORBIDDEN,
                "kek_missing",
                "no per-project benchmark KEK is provisioned; an identified \
                 contribution cannot be written without the key that can later \
                 crypto-shred it"
                    .to_string(),
            ));
        }
        Err(join_err) => {
            tracing::error!(error = %join_err, "contributions: KEK load task panicked");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "failed to load project KEK".to_string(),
            ));
        }
    };

    // 2. Open + AUTHENTICATE the sealed payload. open() verifies the AEAD AAD,
    //    which binds project_hmac + the verification token — a tampered/swapped
    //    token fails here. This is the 403 sealed-payload class (AC-2), distinct
    //    from the upstream 401 HMAC class.
    if let Err(e) = kek.open(&req.sealed) {
        let _ = contributions
            .audit_resolution(&ResolutionAudit {
                project_id,
                verification_token: &verification_token,
                resolved_domain: None,
                claim_status: None,
                decision: ResolutionDecision::SealRejected,
                reason: Some("sealed payload failed AEAD authentication"),
                contribution_id: None,
            })
            .await;
        tracing::warn!(error = %e, "contributions: sealed payload failed to open");
        return Err(err(
            StatusCode::FORBIDDEN,
            "sealed_payload_rejected",
            "the sealed contribution failed authentication (wrong KEK, corrupted \
             ciphertext, or tampered verification token)"
                .to_string(),
        ));
    }

    // 3. Resolve the token → currently-verified brand SERVER-SIDE.
    let outcome = contributions
        .resolve_verified_domain(&verification_token)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "contributions: token resolution failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "failed to resolve verification token".to_string(),
            )
        })?;

    let entity_domain = match outcome {
        ResolutionOutcome::Verified { domain } => domain,
        ResolutionOutcome::Unverified {
            domain,
            claim_status,
        } => {
            let _ = contributions
                .audit_resolution(&ResolutionAudit {
                    project_id,
                    verification_token: &verification_token,
                    resolved_domain: Some(&domain),
                    claim_status: Some(&claim_status),
                    decision: ResolutionDecision::Unverified,
                    reason: Some("domain is not currently claim_status=verified"),
                    contribution_id: None,
                })
                .await;
            return Err(err(
                StatusCode::FORBIDDEN,
                "domain_not_verified",
                format!(
                    "the verification token resolves to a domain whose claim is \
                     `{claim_status}`, not `verified`; identity is refused"
                ),
            ));
        }
        ResolutionOutcome::UnknownToken => {
            let _ = contributions
                .audit_resolution(&ResolutionAudit {
                    project_id,
                    verification_token: &verification_token,
                    resolved_domain: None,
                    claim_status: None,
                    decision: ResolutionDecision::UnknownToken,
                    reason: Some("token matches no verification attempt"),
                    contribution_id: None,
                })
                .await;
            return Err(err(
                StatusCode::FORBIDDEN,
                "unknown_verification_token",
                "the verification token does not resolve to any verified domain".to_string(),
            ));
        }
    };

    // 4. Persist, attributed to the resolved brand via the registry FK (AC-1),
    //    gated on the consent provenance FK (CC-NFR2). A bad consent_record_id or
    //    entity_domain is caught by the FK constraints → 400/409.
    let to_store = IdentifiedContribution {
        project_id,
        project_hmac: req.sealed.project_hmac.clone(),
        consent_record_id: req.consent_record_id,
        verification_token: verification_token.clone(),
        terms_version: anseo_benchmark::TERMS_VERSION.to_string(),
        entity_domain: entity_domain.clone(),
    };
    let contribution_id = contributions.insert(&to_store).await.map_err(|e| {
        if let anseo_storage::Error::Sqlx(sqlx::Error::Database(db_err)) = &e {
            if db_err.code().as_deref() == Some("23503") {
                return err(
                    StatusCode::BAD_REQUEST,
                    "invalid_reference",
                    "consent_record_id or resolved entity does not exist".to_string(),
                );
            }
        }
        tracing::error!(error = %e, "contributions: insert failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "persist_failed",
            "failed to persist identified contribution".to_string(),
        )
    })?;

    // 5. Audit the accepted resolution (CC-NFR2). Best-effort: the contribution
    //    is already durably stored.
    let _ = contributions
        .audit_resolution(&ResolutionAudit {
            project_id,
            verification_token: &verification_token,
            resolved_domain: Some(&entity_domain),
            claim_status: Some("verified"),
            decision: ResolutionDecision::Resolved,
            reason: None,
            contribution_id: Some(contribution_id),
        })
        .await;

    tracing::info!(
        event = "contribution.identified.stored",
        project_hmac = %req.sealed.project_hmac,
        entity_domain = %entity_domain,
        "identified contribution stored with server-resolved brand"
    );

    Ok((
        StatusCode::CREATED,
        Json(ContributionResponse {
            contribution_id: contribution_id.to_string(),
            entity_domain,
            project_id: project_id_str,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_seal_has_no_token_to_resolve() {
        // A request whose sealed contribution carries no verification_token is
        // an anonymous-tier seal and must be rejected at the door (it has no
        // brand to resolve). Guards the BAD_REQUEST branch's premise.
        let store = anseo_core::InMemoryStore::durable_for_tests();
        let kek = ProjectKek::load_or_create(&store, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let payload = anseo_benchmark::Redactor::new(&kek, anseo_benchmark::TERMS_VERSION)
            .redact(anseo_benchmark::RawPromptRun {
                project_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".into(),
                prompt_slug: "vector-db".into(),
                provider: "openai".into(),
                model: "gpt-4o".into(),
                observed_at: chrono::Utc::now(),
                observed_rank: Some(1),
                citation_domains: vec!["docs.example.com".into()],
                brand_name: "Pinecone".into(),
                raw_response_text: String::new(),
                api_key_used: String::new(),
                ip_address: String::new(),
            })
            .unwrap();
        let anon = kek.seal(&payload).unwrap();
        assert!(
            anon.verification_token.is_none(),
            "anonymous seal must carry no token"
        );
    }

    #[test]
    fn decision_strings_are_stable() {
        // The handler maps refusal branches to these audit decisions; lock the
        // mapping so a rename can't silently change the audit ledger contract.
        assert_eq!(ResolutionDecision::KekMissing.as_str(), "kek_missing");
        assert_eq!(ResolutionDecision::SealRejected.as_str(), "seal_rejected");
        assert_eq!(ResolutionDecision::Unverified.as_str(), "unverified");
        assert_eq!(ResolutionDecision::UnknownToken.as_str(), "unknown_token");
        assert_eq!(ResolutionDecision::Resolved.as_str(), "resolved");
    }
}
