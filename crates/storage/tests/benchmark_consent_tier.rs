//! Story 44.1 — benchmark consent-tier semantics (anonymous vs brand-visibility).
//!
//! These DB-backed tests pin the contract the CLI opt-out flow relies on after
//! the 44.1 autoreview fix: a FULL opt-out (which crypto-shreds the shared KEK)
//! must also revoke the brand-visibility (identified) tier, so status never
//! reports the identified tier ACTIVE once the key that backs its contributions
//! is gone.
//!
//! `#[sqlx::test]` is skipped when no DATABASE_URL/test DB is available (local);
//! CI runs them against Postgres.

use anseo_core::BrandConfig;
use anseo_storage::repositories::benchmark_consent::ConsentTier;
use anseo_storage::repositories::entities::EntityRepo;
use anseo_storage::Storage;
use sqlx::PgPool;

const TERMS: &str = "v1-2026-05-28";

fn brand(name: &str) -> BrandConfig {
    BrandConfig {
        name: name.into(),
        variants: vec![format!("{name} Inc")],
        site_url: Some(format!("https://{name}.example")),
    }
}

/// The two tiers are independent: opting in to brand-visibility leaves the
/// anonymous tier untouched, and each reports its own activeness.
#[sqlx::test(migrations = "./migrations")]
async fn tiers_are_independent(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let project = storage
        .projects()
        .create_project(&brand("acme"))
        .await
        .unwrap();
    let repo = storage.benchmark_consent();

    repo.record_optin(project, TERMS, Some("op"), None)
        .await
        .unwrap();
    repo.record_optin_tier(
        project,
        ConsentTier::BrandVisibility,
        TERMS,
        Some("op"),
        None,
    )
    .await
    .unwrap();

    let anon = repo
        .latest_for_tier(project, ConsentTier::Anonymous)
        .await
        .unwrap()
        .unwrap();
    let bv = repo
        .latest_for_tier(project, ConsentTier::BrandVisibility)
        .await
        .unwrap()
        .unwrap();
    assert!(anon.is_active(TERMS));
    assert!(bv.is_active(TERMS));
}

/// Story 44.1 SECURITY gate: the brand-visibility (identified/named) opt-in is a
/// CLAIM and requires a DOMAIN-VERIFIED entry in the registry (Story 43.2). This
/// pins the exact predicate the CLI opt-in path enforces — resolve the project's
/// `brand.site_url` to a normalized domain and require `claim_status ==
/// "verified"` — so an unverified domain is REJECTED and a verified one ACCEPTED.
#[sqlx::test(migrations = "./migrations")]
async fn brand_visibility_requires_domain_verified_claim(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let brand = brand("claimco");
    let domain = EntityRepo::normalize_domain(brand.site_url.as_deref().unwrap());
    let entities = storage.entities();

    // (a) No entity / not verified yet → the gate predicate REJECTS.
    let none = entities.get(&domain).await.unwrap();
    let verified = none.map(|e| e.claim_status == "verified").unwrap_or(false);
    assert!(
        !verified,
        "brand-visibility opt-in must be rejected when the domain is unverified"
    );

    // Register the entity but leave it `pending` — still REJECTED.
    entities.upsert(&domain, "Claim Co", "brand").await.unwrap();
    let pending = entities.get(&domain).await.unwrap().unwrap();
    assert_ne!(
        pending.claim_status, "verified",
        "a pending (unverified) claim must still be rejected"
    );

    // (b) Drive the entity to `verified` → the gate predicate ACCEPTS.
    entities
        .set_claim_status(&domain, "verified", Some("dns_txt"))
        .await
        .unwrap();
    let now_verified = entities.get(&domain).await.unwrap().unwrap();
    assert!(
        now_verified.claim_status == "verified",
        "brand-visibility opt-in must be accepted once the domain is verified"
    );
}

/// Story 44.1 autoreview fix: after a full opt-out appends a brand_visibility
/// optout (the behaviour the CLI now performs when the bv tier is active), the
/// identified tier reports INACTIVE — it can no longer be mistaken for active
/// off a stale optin row once the shared KEK is shredded.
#[sqlx::test(migrations = "./migrations")]
async fn full_optout_revokes_brand_visibility_tier(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let project = storage
        .projects()
        .create_project(&brand("acme"))
        .await
        .unwrap();
    let repo = storage.benchmark_consent();

    // Opt in to both tiers.
    repo.record_optin(project, TERMS, Some("op"), None)
        .await
        .unwrap();
    repo.record_optin_tier(
        project,
        ConsentTier::BrandVisibility,
        TERMS,
        Some("op"),
        None,
    )
    .await
    .unwrap();
    assert!(repo
        .latest_for_tier(project, ConsentTier::BrandVisibility)
        .await
        .unwrap()
        .unwrap()
        .is_active(TERMS));

    // Full opt-out: anonymous optout + (because bv is active) brand_visibility optout.
    repo.record_optout(project, TERMS, Some("op"), None)
        .await
        .unwrap();
    repo.record_optout_tier(
        project,
        ConsentTier::BrandVisibility,
        TERMS,
        Some("op"),
        None,
    )
    .await
    .unwrap();

    // Both tiers now inactive.
    assert!(!repo
        .latest_for_tier(project, ConsentTier::Anonymous)
        .await
        .unwrap()
        .unwrap()
        .is_active(TERMS));
    assert!(
        !repo
            .latest_for_tier(project, ConsentTier::BrandVisibility)
            .await
            .unwrap()
            .unwrap()
            .is_active(TERMS),
        "brand-visibility tier must be inactive after a full opt-out"
    );
}
