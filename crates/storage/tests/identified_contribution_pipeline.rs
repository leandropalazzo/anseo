//! Story 44.2 — identified contribution pipeline + server-side brand resolution.
//!
//! DB-backed contract for the repository leg the API handler drives:
//!   * a token bound to a `verified` domain resolves → stored with the registry
//!     FK (`entity_domain`) and the consent provenance FK (AC-1 / CC-NFR2);
//!   * a token bound to a non-`verified` domain is refused (AC-3);
//!   * an unknown token resolves to nothing (AC-3);
//!   * the raw domain is NOT a cleartext body field — linkage is the FK only.
//!
//! `#[sqlx::test]` is skipped when no test DB is available (local); CI runs them.

use anseo_core::BrandConfig;
use anseo_storage::repositories::benchmark_consent::ConsentTier;
use anseo_storage::repositories::contributions::{
    IdentifiedContribution, ResolutionAudit, ResolutionDecision, ResolutionOutcome,
};
use anseo_storage::repositories::entities::EntityRepo;
use anseo_storage::repositories::verification::VerificationMethod;
use anseo_storage::Storage;
use sqlx::{PgPool, Row};

const TERMS: &str = "v1-2026-05-28";

fn brand(name: &str) -> BrandConfig {
    BrandConfig {
        name: name.into(),
        variants: vec![format!("{name} Inc")],
        site_url: Some(format!("https://{name}.example")),
    }
}

/// Full happy path: verified domain → token resolves → contribution stored with
/// the registry FK + consent FK, and the raw domain is not a cleartext body
/// column (AC-1 / CC-NFR2).
#[sqlx::test(migrations = "./migrations")]
async fn verified_token_resolves_and_stores_with_fk(pool: PgPool) {
    let storage = Storage::from_pool(pool.clone());
    let project = storage
        .projects()
        .create_project(&brand("acme"))
        .await
        .unwrap();
    let domain = EntityRepo::normalize_domain("https://acme.example");

    // Register + verify the domain in the registry.
    let entities = storage.entities();
    entities.upsert(&domain, "Acme", "brand").await.unwrap();
    entities
        .set_claim_status(&domain, "verified", Some("dns_txt"))
        .await
        .unwrap();

    // Mint a verification token bound to that domain, then CONSUME it (the
    // attempt row becomes the spent proof: status='verified', used_at set).
    // Resolution binds to this consumed proof — a merely-pending token must not
    // resolve (see `pending_token_for_verified_domain_does_not_resolve`).
    let minted = storage
        .verification()
        .create_challenge(&domain, VerificationMethod::DnsTxt, Some("sess"), None)
        .await
        .unwrap();
    assert!(
        storage.verification().consume(minted.id).await.unwrap(),
        "minting + consuming the challenge should succeed"
    );

    // Brand-visibility consent (the provenance FK).
    let consent_id = storage
        .benchmark_consent()
        .record_optin_tier(
            project,
            ConsentTier::BrandVisibility,
            TERMS,
            Some("op"),
            None,
        )
        .await
        .unwrap();

    let contributions = storage.contributions();

    // Resolve the token → verified domain.
    let outcome = contributions
        .resolve_verified_domain(&minted.raw_token)
        .await
        .unwrap();
    assert_eq!(
        outcome,
        ResolutionOutcome::Verified {
            domain: domain.clone()
        }
    );

    // Persist.
    let cid = contributions
        .insert(&IdentifiedContribution {
            project_id: project,
            project_hmac: "hmac-acme".into(),
            consent_record_id: consent_id,
            verification_token: minted.raw_token.clone(),
            terms_version: TERMS.into(),
            entity_domain: domain.clone(),
        })
        .await
        .unwrap();

    // Linkage is via the registry FK + the consent FK (AC-1 / CC-NFR2).
    let (entity_domain, consent_fk) = contributions.linkage_for(cid).await.unwrap().unwrap();
    assert_eq!(entity_domain.as_deref(), Some(domain.as_str()));
    assert_eq!(consent_fk, consent_id);

    // The contributions table has NO cleartext free-text "domain"/"brand"
    // column — identity linkage is ONLY the entity_domain FK (AC-1).
    let cols: Vec<String> = sqlx::query(
        "SELECT column_name FROM information_schema.columns WHERE table_name = 'contributions'",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.get::<String, _>("column_name"))
    .collect();
    assert!(cols.contains(&"entity_domain".to_string()));
    assert!(!cols.iter().any(|c| c == "brand_name" || c == "domain"));

    // Audit the resolution (CC-NFR2) and confirm the ledger row lands.
    contributions
        .audit_resolution(&ResolutionAudit {
            project_id: project,
            verification_token: &minted.raw_token,
            resolved_domain: Some(&domain),
            claim_status: Some("verified"),
            decision: ResolutionDecision::Resolved,
            reason: None,
            contribution_id: Some(cid),
        })
        .await
        .unwrap();
    let audit_count: i64 = sqlx::query(
        "SELECT count(*) AS n FROM contribution_resolutions WHERE decision = 'resolved'",
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("n");
    assert_eq!(audit_count, 1);
}

/// A token bound to a domain that is NOT currently `verified` is refused (AC-3),
/// and the refusal is auditable.
#[sqlx::test(migrations = "./migrations")]
async fn unverified_domain_token_is_refused(pool: PgPool) {
    let storage = Storage::from_pool(pool.clone());
    let _project = storage
        .projects()
        .create_project(&brand("pendingco"))
        .await
        .unwrap();
    let domain = EntityRepo::normalize_domain("https://pendingco.example");

    let entities = storage.entities();
    // Registered but left `pending` (not verified) in the registry — e.g. the
    // challenge was consumed once but the claim was later revoked.
    entities
        .upsert(&domain, "Pending Co", "brand")
        .await
        .unwrap();

    // A consumed proof exists (so we bind to a real verified attempt) but the
    // entity's CURRENT claim_status is not 'verified' → still refused (AC-3).
    let minted = storage
        .verification()
        .create_challenge(&domain, VerificationMethod::DnsTxt, Some("sess"), None)
        .await
        .unwrap();
    assert!(storage.verification().consume(minted.id).await.unwrap());

    let outcome = storage
        .contributions()
        .resolve_verified_domain(&minted.raw_token)
        .await
        .unwrap();
    match outcome {
        ResolutionOutcome::Unverified {
            domain: d,
            claim_status,
        } => {
            assert_eq!(d, domain);
            assert_ne!(claim_status, "verified");
        }
        other => panic!("expected Unverified, got {other:?}"),
    }
}

/// A token that matches no verification attempt resolves to nothing (AC-3).
#[sqlx::test(migrations = "./migrations")]
async fn unknown_token_resolves_to_nothing(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let outcome = storage
        .contributions()
        .resolve_verified_domain("totally-unknown-token-value")
        .await
        .unwrap();
    assert_eq!(outcome, ResolutionOutcome::UnknownToken);
}

/// SECURITY (brand-attribution bypass): an authenticated caller starts a fresh
/// DNS-TXT challenge for an ALREADY-verified victim domain and gets a brand-new
/// **pending** token — without ever proving control. That pending token MUST NOT
/// resolve, even though the victim's entity is `claim_status = 'verified'`.
/// Resolution binds to the attempt that actually CONSUMED a token
/// (`status = 'verified'`, `used_at IS NOT NULL`), so the pending token resolves
/// to nothing while the genuinely-consumed token still resolves.
#[sqlx::test(migrations = "./migrations")]
async fn pending_token_for_verified_domain_does_not_resolve(pool: PgPool) {
    let storage = Storage::from_pool(pool.clone());
    let _project = storage
        .projects()
        .create_project(&brand("victim"))
        .await
        .unwrap();
    let domain = EntityRepo::normalize_domain("https://victim.example");

    // The victim domain is legitimately verified in the registry.
    let entities = storage.entities();
    entities.upsert(&domain, "Victim", "brand").await.unwrap();
    entities
        .set_claim_status(&domain, "verified", Some("dns_txt"))
        .await
        .unwrap();

    // The legitimate owner's challenge: minted AND consumed (the real proof).
    let legit = storage
        .verification()
        .create_challenge(
            &domain,
            VerificationMethod::DnsTxt,
            Some("owner-sess"),
            None,
        )
        .await
        .unwrap();
    assert!(storage.verification().consume(legit.id).await.unwrap());

    // The attacker starts a NEW challenge for the same already-verified domain
    // and receives a fresh *pending* token — no DNS-TXT proof was ever submitted.
    // (The live-challenge unique index only blocks a second *pending* row, so the
    // attacker must expire the prior pending one; here the owner's is already
    // consumed, leaving room for this fresh pending challenge.)
    let attacker = storage
        .verification()
        .create_challenge(
            &domain,
            VerificationMethod::DnsTxt,
            Some("attacker-sess"),
            None,
        )
        .await
        .unwrap();
    assert_ne!(attacker.raw_token, legit.raw_token);

    // The attacker's pending token must NOT resolve — no consumed proof backs it.
    let attacker_outcome = storage
        .contributions()
        .resolve_verified_domain(&attacker.raw_token)
        .await
        .unwrap();
    assert_eq!(
        attacker_outcome,
        ResolutionOutcome::UnknownToken,
        "a fresh pending token for a verified domain must not resolve"
    );

    // The legitimately-consumed token still resolves (no regression).
    let legit_outcome = storage
        .contributions()
        .resolve_verified_domain(&legit.raw_token)
        .await
        .unwrap();
    assert_eq!(
        legit_outcome,
        ResolutionOutcome::Verified {
            domain: domain.clone()
        }
    );
}
