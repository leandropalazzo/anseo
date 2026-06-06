//! Identified-contribution persistence + server-side brand resolution
//! (Epic 44 / Story 44.2).
//!
//! This is the SERVER side of the identified tier. The OSS client (44.1)
//! transmits a sealed contribution carrying a **verification token** (43.2) and
//! NEVER a raw brand name. The server:
//!
//!   1. resolves the token → the verified domain via the entity registry
//!      ([`resolve_verified_domain`]), refusing anything that is not currently
//!      `claim_status = 'verified'` (AC-3);
//!   2. persists the identified contribution linked to that domain via the
//!      registry FK (`contributions.entity_domain`) — the raw domain is never a
//!      free-text body field, linkage is the FK only (AC-1);
//!   3. records an append-only audit row for EVERY resolution attempt —
//!      accepted or refused (CC-NFR2 / "audit every resolution").
//!
//! All SQL is dynamic ([`sqlx::query`]); no compile-time `query!` macros.

use anseo_core::ids::ProjectId;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;
use crate::repositories::verification::hash_token;

/// Outcome of resolving a presented verification token to a brand. The token
/// resolves ONLY when it maps to a domain whose entity row is currently
/// `claim_status = 'verified'`. Every other case is a refusal the ingest path
/// turns into a 403 — and every variant is audited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionOutcome {
    /// Token → domain → entity is currently verified. Carries the resolved
    /// (normalized) domain the contribution will be attributed to.
    Verified { domain: String },
    /// Token resolved to a domain, but its entity is NOT currently verified
    /// (unclaimed / pending / revoked / pending_conflict). Carries the domain
    /// and the observed claim_status for the audit trail.
    Unverified {
        domain: String,
        claim_status: String,
    },
    /// The token matches no verification attempt — it resolves to no domain.
    UnknownToken,
}

/// A resolved, ready-to-persist identified contribution. Every field is required
/// to write a row: there is no path that stores an identified contribution
/// without the consent provenance (CC-NFR2) or the resolved brand FK (AC-1).
#[derive(Debug, Clone)]
pub struct IdentifiedContribution {
    pub project_id: ProjectId,
    /// Cleartext linkage HMAC (grouping only — NOT an erasure mechanism).
    pub project_hmac: String,
    /// Consent event that authorized this identified contribution (CC-NFR2).
    pub consent_record_id: Uuid,
    /// The verification token carried by the client. Identity is via this token
    /// only — never a raw brand name.
    pub verification_token: String,
    pub terms_version: String,
    /// The verified domain the token resolved to (registry FK target).
    pub entity_domain: String,
}

/// The audit decision recorded for a resolution attempt. Mirrors the
/// `contribution_resolutions.decision` CHECK in the 44.2 migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionDecision {
    Resolved,
    Unverified,
    UnknownToken,
    SealRejected,
    KekMissing,
}

impl ResolutionDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            ResolutionDecision::Resolved => "resolved",
            ResolutionDecision::Unverified => "unverified",
            ResolutionDecision::UnknownToken => "unknown_token",
            ResolutionDecision::SealRejected => "seal_rejected",
            ResolutionDecision::KekMissing => "kek_missing",
        }
    }
}

/// One row of the append-only resolution audit ledger.
pub struct ResolutionAudit<'a> {
    pub project_id: ProjectId,
    pub verification_token: &'a str,
    pub resolved_domain: Option<&'a str>,
    pub claim_status: Option<&'a str>,
    pub decision: ResolutionDecision,
    pub reason: Option<&'a str>,
    pub contribution_id: Option<Uuid>,
}

pub struct ContributionRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> ContributionRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Resolve a presented verification token to a currently-verified brand.
    ///
    /// The token is hashed (never compared raw) and matched against the
    /// most-recent `verification_attempts` row. If that row's domain has an
    /// entity with `claim_status = 'verified'`, the token resolves; otherwise it
    /// is refused. This is the server-side brand resolution: the client transmits
    /// only the token, the brand is derived here against authoritative state.
    pub async fn resolve_verified_domain(
        &self,
        verification_token: &str,
    ) -> Result<ResolutionOutcome, Error> {
        let token_hash = hash_token(verification_token);

        // Most-recent attempt for this token hash → its domain. We do NOT require
        // the attempt itself to be 'verified' (a token may have been minted and
        // the domain verified out-of-band); the authoritative gate is the
        // entity's CURRENT claim_status, checked next.
        let domain_row = sqlx::query(
            r#"
            SELECT domain
            FROM verification_attempts
            WHERE token_hash = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(self.pool)
        .await?;

        let Some(domain_row) = domain_row else {
            return Ok(ResolutionOutcome::UnknownToken);
        };
        let domain: String = domain_row.get("domain");

        // Authoritative gate: the entity's CURRENT claim_status must be verified.
        let entity_row = sqlx::query(
            r#"
            SELECT claim_status
            FROM entities
            WHERE domain = $1
            "#,
        )
        .bind(&domain)
        .fetch_optional(self.pool)
        .await?;

        let claim_status: String = match entity_row {
            Some(r) => r.get("claim_status"),
            // Token mapped to a domain with no registry entity at all — treat as
            // unverified (cannot attribute to a brand we don't recognize).
            None => {
                return Ok(ResolutionOutcome::Unverified {
                    domain,
                    claim_status: "unknown".to_string(),
                })
            }
        };

        if claim_status == "verified" {
            Ok(ResolutionOutcome::Verified { domain })
        } else {
            Ok(ResolutionOutcome::Unverified {
                domain,
                claim_status,
            })
        }
    }

    /// Persist a resolved identified contribution. Returns the new row id. The
    /// caller must have already resolved the token to a verified domain
    /// ([`resolve_verified_domain`]) and verified the sealed payload — this is
    /// the write leg only.
    pub async fn insert(&self, c: &IdentifiedContribution) -> Result<Uuid, Error> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO contributions
                (id, project_id, project_hmac, consent_record_id,
                 verification_token, terms_version, entity_domain)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(id)
        .bind(c.project_id)
        .bind(&c.project_hmac)
        .bind(c.consent_record_id)
        .bind(&c.verification_token)
        .bind(&c.terms_version)
        .bind(&c.entity_domain)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Append a resolution-audit row (CC-NFR2). Records EVERY decision —
    /// accepted and refused alike — with the token hash (never the raw token).
    pub async fn audit_resolution(&self, a: &ResolutionAudit<'_>) -> Result<Uuid, Error> {
        let id = Uuid::new_v4();
        let token_hash = hash_token(a.verification_token);
        sqlx::query(
            r#"
            INSERT INTO contribution_resolutions
                (id, project_id, token_hash, resolved_domain, claim_status,
                 decision, reason, contribution_id)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(a.project_id)
        .bind(&token_hash)
        .bind(a.resolved_domain)
        .bind(a.claim_status)
        .bind(a.decision.as_str())
        .bind(a.reason)
        .bind(a.contribution_id)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Fetch a stored contribution's `(entity_domain, consent_record_id)` for
    /// test/assertion of the registry-FK linkage (AC-1 / CC-NFR2).
    pub async fn linkage_for(
        &self,
        contribution_id: Uuid,
    ) -> Result<Option<(Option<String>, Uuid)>, Error> {
        let row = sqlx::query(
            r#"
            SELECT entity_domain, consent_record_id
            FROM contributions
            WHERE id = $1
            "#,
        )
        .bind(contribution_id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| {
            (
                r.get::<Option<String>, _>("entity_domain"),
                r.get::<Uuid, _>("consent_record_id"),
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_as_str_matches_check_constraint() {
        assert_eq!(ResolutionDecision::Resolved.as_str(), "resolved");
        assert_eq!(ResolutionDecision::Unverified.as_str(), "unverified");
        assert_eq!(ResolutionDecision::UnknownToken.as_str(), "unknown_token");
        assert_eq!(ResolutionDecision::SealRejected.as_str(), "seal_rejected");
        assert_eq!(ResolutionDecision::KekMissing.as_str(), "kek_missing");
    }

    #[test]
    fn resolution_outcome_distinguishes_verified_from_refusals() {
        let v = ResolutionOutcome::Verified {
            domain: "example.com".into(),
        };
        let u = ResolutionOutcome::Unverified {
            domain: "example.com".into(),
            claim_status: "pending".into(),
        };
        assert_ne!(v, u);
        assert_ne!(v, ResolutionOutcome::UnknownToken);
    }
}
