//! Story 43.2 (AC-5) — daily DNS re-verification / revocation job.
//!
//! `run_reverification_job` re-checks every dns_txt-verified domain. When the
//! challenge TXT record is gone it revokes the entity (grace period starts) and
//! appends a revocation ledger row; when the record is still present it leaves
//! the entity verified. These tests drive the job with a [`MockTxtResolver`]
//! (no network) against a real Postgres so the SQL paths are exercised.

use anseo_storage::repositories::verification::{
    run_reverification_job, MintedChallenge, MockTxtResolver, VerificationMethod,
};
use anseo_storage::Storage;
use sqlx::PgPool;

/// Register an entity and drive it to a `verified` dns_txt state, returning the
/// raw token that was published in DNS so the caller can decide whether the
/// resolver still serves it.
async fn make_dns_verified(storage: &Storage, domain: &str) -> String {
    storage
        .entities()
        .upsert(domain, domain, "brand")
        .await
        .unwrap();

    let challenge = storage
        .verification()
        .create_challenge(domain, VerificationMethod::DnsTxt, None, None)
        .await
        .unwrap();
    // Consume the single-use challenge → state = verified.
    assert!(storage.verification().consume(challenge.id).await.unwrap());
    storage
        .entities()
        .set_claim_status(domain, "verified", Some("dns_txt"))
        .await
        .unwrap();

    challenge.raw_token
}

// AC-5: a verified entity whose TXT record is GONE is revoked + grace started.
#[sqlx::test(migrations = "./migrations")]
async fn revokes_when_txt_record_removed(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let _token = make_dns_verified(&storage, "gone.example").await;

    // Resolver serves NOTHING for the challenge name → record removed.
    let resolver = MockTxtResolver::new();
    let revoked = run_reverification_job(&storage, &resolver).await.unwrap();
    assert_eq!(
        revoked, 1,
        "the domain with a removed TXT record is revoked"
    );

    let entity = storage
        .entities()
        .get("gone.example")
        .await
        .unwrap()
        .expect("entity exists");
    assert_eq!(entity.claim_status, "revoked");
}

// AC-5: a verified entity whose TXT record is STILL PRESENT stays verified.
#[sqlx::test(migrations = "./migrations")]
async fn keeps_verified_when_txt_record_present(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    let token = make_dns_verified(&storage, "stays.example").await;

    // Resolver still serves the exact challenge value at the challenge name.
    let name = MintedChallenge::dns_record_name("stays.example");
    let resolver = MockTxtResolver::new().with_record(&name, &format!("anseo-verify={token}"));

    let revoked = run_reverification_job(&storage, &resolver).await.unwrap();
    assert_eq!(revoked, 0, "a still-present record is not revoked");

    let entity = storage
        .entities()
        .get("stays.example")
        .await
        .unwrap()
        .expect("entity exists");
    assert_eq!(entity.claim_status, "verified");
}
