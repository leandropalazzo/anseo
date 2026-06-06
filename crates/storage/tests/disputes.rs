//! Story 43.6 — disputes lifecycle integration tests.
//!
//! Exercises the operator review workflow (AC-1), claim-conflict adjudication
//! with DNS-TXT as arbiter + losing-party notification (AC-2), GDPR Art.21
//! assessment audit (AC-3), and removal/suppression (AC-4). Every test asserts
//! the audit trail is written (NFR5).

use anseo_storage::Storage;
use sqlx::PgPool;

async fn register(storage: &Storage, domain: &str, name: &str) {
    storage
        .entities()
        .upsert(domain, name, "brand")
        .await
        .unwrap();
}

// AC-1: approve correction updates the registry + logs the decision.
#[sqlx::test(migrations = "./migrations")]
async fn approve_correction_updates_registry_and_audits(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    register(&storage, "acme.com", "Acme Inc").await;

    let d = storage
        .disputes()
        .submit(
            "acme.com",
            "correction",
            "Name is wrong",
            Some("user@acme.com"),
            Some("Acme Corporation"),
        )
        .await
        .unwrap();
    assert_eq!(d.status, "open");

    let resolved = storage
        .disputes()
        .approve_correction(d.id, "op1", "verified against trademark filing")
        .await
        .unwrap();
    assert_eq!(resolved.status, "approved");
    assert_eq!(resolved.resolved_by.as_deref(), Some("op1"));

    // Registry display_name updated.
    let entity = storage.entities().get("acme.com").await.unwrap().unwrap();
    assert_eq!(entity.display_name, "Acme Corporation");

    // Audit trail: submitted + approved.
    let events = storage.disputes().events(d.id).await.unwrap();
    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert!(types.contains(&"submitted"));
    assert!(types.contains(&"approved"));
}

// AC-1: reject records grounds + appeals path; registry untouched.
#[sqlx::test(migrations = "./migrations")]
async fn reject_records_grounds_and_audits(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    register(&storage, "beta.com", "Beta LLC").await;

    let d = storage
        .disputes()
        .submit("beta.com", "correction", "change it", None, Some("Bogus"))
        .await
        .unwrap();

    let rejected = storage
        .disputes()
        .reject(d.id, "op2", "no evidence provided; reply to appeal")
        .await
        .unwrap();
    assert_eq!(rejected.status, "rejected");
    assert_eq!(
        rejected.resolution_grounds.as_deref(),
        Some("no evidence provided; reply to appeal")
    );

    // Registry unchanged.
    let entity = storage.entities().get("beta.com").await.unwrap().unwrap();
    assert_eq!(entity.display_name, "Beta LLC");

    let events = storage.disputes().events(d.id).await.unwrap();
    assert!(events.iter().any(|e| e.event_type == "rejected"));
}

// AC-2: DNS-TXT proof decides; winner verified, loser notified.
#[sqlx::test(migrations = "./migrations")]
async fn claim_conflict_winner_verified_loser_notified(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    register(&storage, "contested.com", "Contested").await;

    let d = storage
        .disputes()
        .submit(
            "contested.com",
            "claim_conflict",
            "two claimants",
            None,
            None,
        )
        .await
        .unwrap();

    let resolved = storage
        .disputes()
        .adjudicate_claim_conflict(
            d.id,
            "op3",
            "winner@contested.com",
            Some("loser@other.com"),
            "winner produced DNS-TXT record",
        )
        .await
        .unwrap();
    assert_eq!(resolved.status, "resolved");

    // Domain-control arbiter → entity verified via dns_txt.
    let entity = storage
        .entities()
        .get("contested.com")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entity.claim_status, "verified");
    assert_eq!(entity.verification_method.as_deref(), Some("dns_txt"));

    // Losing party notification recorded with re-claim option.
    let events = storage.disputes().events(d.id).await.unwrap();
    let notif = events
        .iter()
        .find(|e| e.event_type == "notification_sent")
        .expect("losing party notification");
    assert_eq!(notif.detail["recipient"], "loser@other.com");
    assert_eq!(notif.detail["reclaim_available"], true);
    assert!(events
        .iter()
        .any(|e| e.event_type == "conflict_adjudicated"));
}

// AC-3: GDPR Art.21 honored → suppressed + outcome stored in audit log.
#[sqlx::test(migrations = "./migrations")]
async fn gdpr_objection_honored_suppresses_and_audits(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    register(&storage, "person.example", "Jane Doe").await;

    let d = storage
        .disputes()
        .submit(
            "person.example",
            "gdpr_objection",
            "remove my data",
            Some("jane@x.com"),
            None,
        )
        .await
        .unwrap();

    let resolved = storage
        .disputes()
        .assess_gdpr_objection(
            d.id,
            "dpo",
            true,
            "genuinely personal; no overriding grounds",
        )
        .await
        .unwrap();
    assert_eq!(resolved.status, "approved");
    assert!(resolved.suppressed);
    assert!(storage
        .disputes()
        .is_suppressed("person.example")
        .await
        .unwrap());

    let events = storage.disputes().events(d.id).await.unwrap();
    let assessed = events
        .iter()
        .find(|e| e.event_type == "gdpr_assessed")
        .expect("gdpr assessment event");
    assert_eq!(assessed.detail["outcome"], "honored");
    assert_eq!(assessed.detail["processing_stopped"], true);
}

// AC-3: GDPR Art.21 refused → not suppressed; grounds documented.
#[sqlx::test(migrations = "./migrations")]
async fn gdpr_objection_refused_documents_grounds(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    register(&storage, "corp.example", "BigCorp").await;

    let d = storage
        .disputes()
        .submit("corp.example", "gdpr_objection", "remove", None, None)
        .await
        .unwrap();

    let resolved = storage
        .disputes()
        .assess_gdpr_objection(d.id, "dpo", false, "not personal data; corporate entity")
        .await
        .unwrap();
    assert_eq!(resolved.status, "rejected");
    assert!(!resolved.suppressed);

    let events = storage.disputes().events(d.id).await.unwrap();
    let assessed = events
        .iter()
        .find(|e| e.event_type == "gdpr_assessed")
        .unwrap();
    assert_eq!(assessed.detail["outcome"], "refused");
}

// AC-4: removal request auto-suppresses pending review; audited.
#[sqlx::test(migrations = "./migrations")]
async fn removal_request_auto_suppresses_pending_review(pool: PgPool) {
    let storage = Storage::from_pool(pool);

    let d = storage
        .disputes()
        .submit("takedown.example", "removal", "please remove", None, None)
        .await
        .unwrap();
    assert!(d.suppressed, "removal must suppress on submit (AC-4)");
    assert_eq!(d.status, "open");
    assert!(storage
        .disputes()
        .is_suppressed("takedown.example")
        .await
        .unwrap());

    let events = storage.disputes().events(d.id).await.unwrap();
    assert!(events.iter().any(|e| e.event_type == "suppressed"));
}

// Change-of-control: new owner takes the verified claim; transfer audited.
#[sqlx::test(migrations = "./migrations")]
async fn change_of_control_transfers_and_audits(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    register(&storage, "acquired.com", "Acquired Co").await;

    let d = storage
        .disputes()
        .submit(
            "acquired.com",
            "change_of_control",
            "we bought the domain",
            None,
            None,
        )
        .await
        .unwrap();

    let resolved = storage
        .disputes()
        .transfer_control(
            d.id,
            "op4",
            "newowner@acquired.com",
            "DNS-TXT re-proved by acquirer",
        )
        .await
        .unwrap();
    assert_eq!(resolved.status, "resolved");

    let entity = storage
        .entities()
        .get("acquired.com")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entity.claim_status, "verified");

    let events = storage.disputes().events(d.id).await.unwrap();
    let transfer = events
        .iter()
        .find(|e| e.event_type == "control_transferred")
        .expect("transfer event");
    assert_eq!(transfer.detail["new_owner_email"], "newowner@acquired.com");
}

// The operator review queue surfaces open + under_review disputes.
#[sqlx::test(migrations = "./migrations")]
async fn pending_queue_lists_open_disputes(pool: PgPool) {
    let storage = Storage::from_pool(pool);
    register(&storage, "one.com", "One").await;
    register(&storage, "two.com", "Two").await;

    let d1 = storage
        .disputes()
        .submit("one.com", "correction", "x", None, Some("One!"))
        .await
        .unwrap();
    let _d2 = storage
        .disputes()
        .submit("two.com", "correction", "y", None, Some("Two!"))
        .await
        .unwrap();

    let pending = storage.disputes().pending().await.unwrap();
    assert_eq!(pending.len(), 2);

    // Resolving one removes it from the queue.
    storage.disputes().reject(d1.id, "op", "no").await.unwrap();
    let pending = storage.disputes().pending().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].domain, "two.com");
}
