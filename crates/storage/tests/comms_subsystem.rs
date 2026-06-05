//! Story 43.7 — comms subsystem DB integration tests.
//!
//! Exercises the `anseo_comms::repo::CommsRepo` against the embedded
//! migrations. These require Postgres (`#[sqlx::test]` provisions a scratch DB)
//! and are skipped locally when `DATABASE_URL` is unset — that's expected; CI
//! provides Postgres.
//!
//! Coverage maps to the story's test obligations:
//!   * Unsubscribe: opt-out stored → subsequent dispatch skipped.
//!   * GDPR Art.21(2): EU objection → immediate suppression, no confirm gate.
//!   * EU consent gate: marketing_consent = false → marketing skipped.
//!   * Preference center toggles persist.
//!   * Magic-link idempotency: a 'sent' dedup_key blocks a second send.

use anseo_comms::dispatch::{marketing_decision, MarketingCategory, MarketingDecision};
use anseo_comms::recipient::recipient_hash;
use anseo_comms::repo::{CommsRepo, PreferenceUpdate, SendOutcome, SuppressionReason};
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn unsubscribe_suppresses_subsequent_marketing(pool: PgPool) {
    let repo = CommsRepo::new(&pool);
    let rh = recipient_hash("owner@acme.com");
    repo.upsert_subscription(&rh, "owner@acme.com", false)
        .await
        .unwrap();

    // Before unsubscribe: not suppressed → dispatch would proceed.
    assert!(!repo.is_marketing_suppressed(&rh).await.unwrap());
    let sub = repo.get_subscription(&rh).await.unwrap().unwrap();
    assert_eq!(
        marketing_decision(false, &sub, MarketingCategory::RankChange),
        MarketingDecision::Send
    );

    // One-click unsubscribe.
    repo.suppress(&rh, SuppressionReason::Unsubscribe, "marketing")
        .await
        .unwrap();

    // After: suppressed → dispatch skipped.
    assert!(repo.is_marketing_suppressed(&rh).await.unwrap());
    assert_eq!(
        marketing_decision(true, &sub, MarketingCategory::RankChange),
        MarketingDecision::SuppressedByList
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn gdpr_objection_is_immediate(pool: PgPool) {
    let repo = CommsRepo::new(&pool);
    let rh = recipient_hash("eu@acme.de");
    repo.upsert_subscription(&rh, "eu@acme.de", true)
        .await
        .unwrap();

    // GDPR Art.21(2): a single suppress call with scope 'all' takes effect
    // immediately — no confirmation row, no pending state.
    repo.suppress(&rh, SuppressionReason::GdprObjection, "all")
        .await
        .unwrap();
    assert!(repo.is_marketing_suppressed(&rh).await.unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn eu_consent_gate_blocks_without_opt_in(pool: PgPool) {
    let repo = CommsRepo::new(&pool);
    let rh = recipient_hash("eu2@acme.fr");
    // EU resident, consent defaults to false.
    let sub = repo
        .upsert_subscription(&rh, "eu2@acme.fr", true)
        .await
        .unwrap();
    assert!(sub.is_eu_resident);
    assert!(!sub.marketing_consent);
    assert_eq!(
        marketing_decision(false, &sub, MarketingCategory::RankChange),
        MarketingDecision::BlockedByEuConsent
    );

    // Grant consent → now allowed.
    let updated = repo
        .update_preferences(
            &rh,
            &PreferenceUpdate {
                marketing_consent: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        marketing_decision(false, &updated, MarketingCategory::RankChange),
        MarketingDecision::Send
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn preference_toggles_persist(pool: PgPool) {
    let repo = CommsRepo::new(&pool);
    let rh = recipient_hash("p@acme.com");
    repo.upsert_subscription(&rh, "p@acme.com", false)
        .await
        .unwrap();

    repo.update_preferences(
        &rh,
        &PreferenceUpdate {
            rank_change_enabled: Some(false),
            digest_frequency: Some("monthly".into()),
            all_marketing_off: Some(false),
            marketing_consent: None,
        },
    )
    .await
    .unwrap();

    let sub = repo.get_subscription(&rh).await.unwrap().unwrap();
    assert!(!sub.rank_change_enabled);
    assert_eq!(sub.digest_frequency, "monthly");
}

#[sqlx::test(migrations = "./migrations")]
async fn token_resolves_to_recipient(pool: PgPool) {
    let repo = CommsRepo::new(&pool);
    let rh = recipient_hash("t@acme.com");
    repo.upsert_subscription(&rh, "t@acme.com", false)
        .await
        .unwrap();
    let minted = anseo_comms::token::mint(b"deployment-secret", &rh, "nonce-1");
    repo.store_token(&minted.hash, &rh, None).await.unwrap();

    let resolved = repo
        .resolve_token(&anseo_comms::token::hash_token(&minted.raw))
        .await
        .unwrap();
    assert_eq!(resolved.as_deref(), Some(rh.as_str()));

    // Unknown token → None.
    assert!(repo.resolve_token("deadbeef").await.unwrap().is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn magic_link_dedup_blocks_second_send(pool: PgPool) {
    let repo = CommsRepo::new(&pool);
    let rh = recipient_hash("m@acme.com");
    let dedup = "token-hash-xyz";

    assert!(!repo.already_sent_dedup(dedup).await.unwrap());
    repo.log_send(
        &rh,
        "transactional",
        "domain_verification",
        SendOutcome::Sent,
        None,
        Some(dedup),
    )
    .await
    .unwrap();
    // AC-5: a magic-link is not retried more than once within the window.
    assert!(repo.already_sent_dedup(dedup).await.unwrap());
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_send_is_audited_with_error(pool: PgPool) {
    let repo = CommsRepo::new(&pool);
    let rh = recipient_hash("f@acme.com");
    // AC-5: failures are logged with recipient (hashed), type, and error.
    repo.log_send(
        &rh,
        "marketing",
        "rank_change",
        SendOutcome::Failed,
        Some("smtp 451 temporary failure"),
        None,
    )
    .await
    .unwrap();

    let row = sqlx::query_scalar::<_, String>(
        "SELECT error FROM comms_send_log WHERE recipient_hash = $1 AND outcome = 'failed'",
    )
    .bind(&rh)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, "smtp 451 temporary failure");
    // Recipient is stored hashed, never cleartext.
    assert!(!rh.contains("acme"));
}
