//! Phase 2 Story 12.4 — live-Postgres integration tests for the
//! orchestration tick (P0-127, P1-128, P1-129, P1-130).
//!
//! Gated behind the `live_db_tests` Cargo feature so `cargo test
//! --workspace` stays green without infrastructure. CI flips the
//! feature on, sets `DATABASE_URL`, and runs `cargo test
//! --features live_db_tests`.

#![cfg(feature = "live_db_tests")]

use std::time::Duration;

use chrono::Utc;
use opengeo_core::{api_key::sha256_hex, ProjectId};
use opengeo_scheduler::webhooks::poller::poll_once;
use opengeo_scheduler::webhooks::signer::{verify, SIGNATURE_HEADER};
use opengeo_scheduler::webhooks::tick::DispatchResult;
use opengeo_storage::Storage;
use serde_json::json;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

async fn fresh_storage() -> Storage {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let storage = Storage::connect(&url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    storage
}

/// Seed a project + webhook row with a known secret and target URL.
/// Returns (project_uuid, webhook_uuid).
async fn seed_project_and_webhook(
    pool: &PgPool,
    target_url: &str,
    secret: &[u8],
    event_kinds: &[&str],
) -> (Uuid, Uuid) {
    let project_id = ProjectId::new();
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
        .bind(project_uuid)
        .bind(format!("test-{}", project_uuid))
        .execute(pool)
        .await
        .expect("insert project");

    let webhook_id = Uuid::from_u128(ulid::Ulid::new().0);
    let kinds_jsonb = json!(event_kinds);
    let secret_b64 = base64_encode(secret);
    sqlx::query(
        r#"INSERT INTO webhooks
           (id, project_id, name, target_url, secret_ciphertext, event_kinds)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(webhook_id)
    .bind(project_uuid)
    .bind(format!("test-webhook-{webhook_id}"))
    .bind(target_url)
    .bind(secret_b64)
    .bind(kinds_jsonb)
    .execute(pool)
    .await
    .expect("insert webhook");

    (project_uuid, webhook_id)
}

fn base64_encode(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(A[(b0 >> 2) as usize] as char);
        out.push(A[(((b0 & 3) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(A[(((b1 & 15) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(A[(b2 & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[tokio::test]
#[serial_test::serial]
async fn p0_127_signature_round_trip_through_dispatcher_against_live_consumer() {
    // P0-127: dispatcher signs the body; the consumer's reference verify
    // (using the same secret) succeeds. End-to-end signature contract.
    let storage = fresh_storage().await;
    let mock = MockServer::start().await;
    let secret = b"webhook-test-secret-bytes-32-len";
    let payload = json!({"event_kind": "prompt_run.completed", "event_id": "01H"});

    let captured: Arc<Mutex<Option<(String, Vec<u8>)>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(move |req: &Request| {
            let mut c = captured_clone.lock().unwrap();
            let sig = req
                .headers
                .get(SIGNATURE_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(String::from)
                .expect("X-OpenGEO-Signature header present");
            *c = Some((sig, req.body.clone()));
            ResponseTemplate::new(204)
        })
        .mount(&mock)
        .await;

    let target = format!("{}/hook", mock.uri());
    let (_project, webhook_id) =
        seed_project_and_webhook(storage.pool(), &target, secret, &["prompt_run.completed"]).await;

    let event_id = Uuid::from_u128(ulid::Ulid::new().0);
    storage
        .webhook_deliveries()
        .insert_pending(
            webhook_id,
            event_id,
            "prompt_run.completed",
            1,
            None,
            &payload,
        )
        .await
        .expect("insert delivery");

    let http = reqwest::Client::new();
    let results = poll_once(&storage, &http, 10, Duration::from_secs(5))
        .await
        .expect("poll_once");
    assert_eq!(results, vec![DispatchResult::Delivered]);

    let cap = captured.lock().unwrap();
    let (sig_header, body) = cap.as_ref().expect("consumer captured request");
    verify(
        secret,
        body,
        Some(sig_header),
        Utc::now().timestamp() + 30,
        300,
    )
    .expect("signer::verify must pass against same secret + body");
}

#[tokio::test]
#[serial_test::serial]
async fn p1_128_retry_ladder_advances_attempt_and_next_attempt_at_on_5xx() {
    // P1-128: a 5xx response advances `attempt` and writes
    // `next_attempt_at` per the retry ladder. The row stays `pending`
    // until the next attempt.
    let storage = fresh_storage().await;
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock)
        .await;

    let target = format!("{}/hook", mock.uri());
    let (_project, webhook_id) = seed_project_and_webhook(
        storage.pool(),
        &target,
        b"secret",
        &["prompt_run.completed"],
    )
    .await;

    let event_id = Uuid::from_u128(ulid::Ulid::new().0);
    let delivery_id = storage
        .webhook_deliveries()
        .insert_pending(
            webhook_id,
            event_id,
            "prompt_run.completed",
            1,
            None,
            &json!({}),
        )
        .await
        .expect("insert");

    let http = reqwest::Client::new();
    let results = poll_once(&storage, &http, 10, Duration::from_secs(5))
        .await
        .expect("poll");
    assert_eq!(results, vec![DispatchResult::Retrying]);

    // Row should now be 'failed' (retryable) with next_attempt_at set.
    let row = sqlx::query_as::<_, (String, Option<chrono::DateTime<Utc>>)>(
        r#"SELECT status, next_attempt_at FROM webhook_deliveries WHERE id = $1"#,
    )
    .bind(delivery_id)
    .fetch_one(storage.pool())
    .await
    .expect("fetch row");
    assert_eq!(row.0, "failed", "status after retryable 5xx");
    let next_at = row.1.expect("next_attempt_at populated");
    // First-attempt retry is 1 second after now (LADDER[0]).
    let delta = (next_at - Utc::now()).num_milliseconds();
    assert!(
        (0..=2_000).contains(&delta),
        "next_attempt_at should be ~1s in the future, got delta_ms={delta}"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn p1_129_auto_disable_after_five_consecutive_permanent_failures() {
    // P1-129: 5 consecutive permanent-failed deliveries on the same
    // webhook flip `disabled = true` with a descriptive reason.
    let storage = fresh_storage().await;
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&mock)
        .await;

    let target = format!("{}/hook", mock.uri());
    let (_project, webhook_id) = seed_project_and_webhook(
        storage.pool(),
        &target,
        b"secret",
        &["prompt_run.completed"],
    )
    .await;

    let http = reqwest::Client::new();
    // Insert + dispatch 5 deliveries; each one drops permanently (400).
    for i in 0..5 {
        let event_id = Uuid::from_u128(ulid::Ulid::new().0);
        storage
            .webhook_deliveries()
            .insert_pending(
                webhook_id,
                event_id,
                "prompt_run.completed",
                1,
                None,
                &json!({"i": i}),
            )
            .await
            .expect("insert");
    }

    let results = poll_once(&storage, &http, 10, Duration::from_secs(5))
        .await
        .expect("poll");
    assert_eq!(results.len(), 5);
    // The last dispatch should report the auto-disable transition.
    assert!(
        results
            .iter()
            .any(|r| matches!(r, DispatchResult::DroppedAndWebhookAutoDisabled)),
        "expected one DroppedAndWebhookAutoDisabled in results, got: {results:?}"
    );

    // Webhook row should now be disabled.
    let row = sqlx::query_as::<_, (bool, Option<String>)>(
        r#"SELECT disabled, disabled_reason FROM webhooks WHERE id = $1"#,
    )
    .bind(webhook_id)
    .fetch_one(storage.pool())
    .await
    .expect("fetch webhook");
    assert!(row.0, "webhook must be disabled after 5 permanent failures");
    let reason = row.1.expect("disabled_reason populated");
    assert!(reason.contains("auto-disabled"));
    assert!(reason.contains("5"));
}

#[tokio::test]
#[serial_test::serial]
async fn p1_130_failure_isolation_per_webhook_target() {
    // P1-130: one always-failing webhook target does not delay sibling
    // delivery for a healthy second target. We seed two webhooks for
    // the same event; one returns 500, one returns 204; both rows must
    // resolve in the same poll sweep.
    let storage = fresh_storage().await;

    let healthy_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/healthy"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&healthy_mock)
        .await;

    let failing_mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/down"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&failing_mock)
        .await;

    let healthy_target = format!("{}/healthy", healthy_mock.uri());
    let failing_target = format!("{}/down", failing_mock.uri());

    let project_id = ProjectId::new();
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
        .bind(project_uuid)
        .bind(format!("isolation-{}", project_uuid))
        .execute(storage.pool())
        .await
        .expect("project");

    let kinds = json!(["prompt_run.completed"]);

    let healthy_webhook = Uuid::from_u128(ulid::Ulid::new().0);
    sqlx::query(
        r#"INSERT INTO webhooks (id, project_id, name, target_url, secret_ciphertext, event_kinds)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(healthy_webhook)
    .bind(project_uuid)
    .bind("healthy")
    .bind(&healthy_target)
    .bind("c2VjcmV0") // base64("secret")
    .bind(&kinds)
    .execute(storage.pool())
    .await
    .expect("healthy wh");

    let failing_webhook = Uuid::from_u128(ulid::Ulid::new().0);
    sqlx::query(
        r#"INSERT INTO webhooks (id, project_id, name, target_url, secret_ciphertext, event_kinds)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(failing_webhook)
    .bind(project_uuid)
    .bind("failing")
    .bind(&failing_target)
    .bind("c2VjcmV0")
    .bind(&kinds)
    .execute(storage.pool())
    .await
    .expect("failing wh");

    let event_id = Uuid::from_u128(ulid::Ulid::new().0);
    let payload = json!({"event_kind": "prompt_run.completed"});
    storage
        .webhook_deliveries()
        .insert_pending(
            healthy_webhook,
            event_id,
            "prompt_run.completed",
            1,
            None,
            &payload,
        )
        .await
        .unwrap();
    storage
        .webhook_deliveries()
        .insert_pending(
            failing_webhook,
            event_id,
            "prompt_run.completed",
            1,
            None,
            &payload,
        )
        .await
        .unwrap();

    let http = reqwest::Client::new();
    let started = std::time::Instant::now();
    let results = poll_once(&storage, &http, 10, Duration::from_secs(5))
        .await
        .expect("poll");
    let elapsed = started.elapsed();

    // Both rows resolved in one sweep — order isn't guaranteed but
    // length is.
    assert_eq!(
        results.len(),
        2,
        "both rows should resolve, got {results:?}"
    );
    let healthy = results
        .iter()
        .filter(|r| matches!(r, DispatchResult::Delivered))
        .count();
    let failing = results
        .iter()
        .filter(|r| r.is_retrying_or_dropped())
        .count();
    assert_eq!(healthy, 1);
    assert_eq!(failing, 1);

    // The failing target shouldn't have blocked the healthy one for
    // longer than ~2 seconds (per-task isolation).
    assert!(
        elapsed < Duration::from_secs(5),
        "sweep took too long: {elapsed:?}"
    );
}

trait DispatchResultExt {
    fn is_retrying_or_dropped(&self) -> bool;
}

impl DispatchResultExt for DispatchResult {
    fn is_retrying_or_dropped(&self) -> bool {
        matches!(
            self,
            DispatchResult::Retrying
                | DispatchResult::DroppedPermanent
                | DispatchResult::DroppedAndWebhookAutoDisabled
        )
    }
}

// Story 12.1 api_keys live-DB round trip (compile-time fence for the
// hot path the auth middleware depends on).
#[tokio::test]
#[serial_test::serial]
async fn story_12_1_api_key_insert_lookup_revoke_round_trip() {
    let storage = fresh_storage().await;
    let project_id = ProjectId::new();
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
        .bind(project_uuid)
        .bind(format!("api-key-{}", project_uuid))
        .execute(storage.pool())
        .await
        .expect("project");

    let key = opengeo_core::api_key::generate();
    let hash = sha256_hex(&key.plaintext);
    storage
        .api_keys()
        .insert(project_id, "ci-bot", &hash, &key.display_prefix)
        .await
        .expect("insert");

    // Lookup hits.
    let found = storage
        .api_keys()
        .lookup_active_project(&hash)
        .await
        .expect("lookup");
    assert_eq!(found, Some(project_id));

    // Count-for-project = 1.
    let count = storage
        .api_keys()
        .count_active_for_project(project_id)
        .await
        .expect("count");
    assert_eq!(count, 1);

    // Revoke + lookup should miss.
    let revoked = storage
        .api_keys()
        .revoke(project_id, "ci-bot", Some("rotation"))
        .await
        .expect("revoke");
    assert!(revoked);
    let found_after_revoke = storage
        .api_keys()
        .lookup_active_project(&hash)
        .await
        .expect("lookup after revoke");
    assert_eq!(found_after_revoke, None);
}
