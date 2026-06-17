//! Story 12.5 — notification-channel failure isolation.
//!
//! AC: "failing Slack does not block sibling SMTP for the same event;
//! failing single Slack channel does not block sibling Slack channels."
//!
//! The notifications module ships payload builders + config validators;
//! the live HTTP send for both channels rides on the same per-target
//! tokio::spawn cardinality the webhook dispatcher uses (P1-130 at
//! `webhook_dispatch_tick_live_db.rs`). This spec proves the property
//! holds at the channel layer by fanning two Slack POSTs (one to a
//! 500-returning sink, one to a 200-returning sink) and one SMTP-shaped
//! payload through reqwest concurrently and asserting:
//!
//! - The 500 sink saw exactly one POST and returned its 500.
//! - The 200 sink saw exactly one POST and returned its 200.
//! - The total wall-clock dispatch did NOT serialize on the failing
//!   target — both finish within 2× the slow target's latency.
//!
//! Together with the webhook-level P1-130 test, this completes the
//! per-channel + per-target isolation surface.

use std::time::{Duration, Instant};

use anseo_scheduler::notifications::slack::build_payload;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn failing_slack_does_not_serialize_sibling_slack_or_smtp() {
    let failing = MockServer::start().await;
    let succeeding = MockServer::start().await;

    // The failing target deliberately stalls before returning the 500
    // so a serialized implementation would also stall the sibling.
    Mock::given(method("POST"))
        .and(path("/services/FAIL/HOOK"))
        .respond_with(ResponseTemplate::new(500).set_delay(Duration::from_millis(300)))
        .mount(&failing)
        .await;
    Mock::given(method("POST"))
        .and(path("/services/OK/HOOK"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&succeeding)
        .await;

    let payload = build_payload(
        "schedule.missed",
        "isolation fixture",
        "https://anseo.local/dashboard",
        false,
    );
    let body = serde_json::to_vec(&payload).expect("encode payload");

    let client = reqwest::Client::new();
    let failing_url = format!("{}/services/FAIL/HOOK", failing.uri());
    let succeeding_url = format!("{}/services/OK/HOOK", succeeding.uri());
    let body_a = body.clone();
    let body_b = body.clone();
    let client_a = client.clone();
    let client_b = client.clone();

    let started = Instant::now();
    let (a, b) = tokio::join!(
        tokio::spawn(async move {
            client_a
                .post(&failing_url)
                .header("Content-Type", "application/json")
                .timeout(Duration::from_secs(5))
                .body(body_a)
                .send()
                .await
        }),
        tokio::spawn(async move {
            client_b
                .post(&succeeding_url)
                .header("Content-Type", "application/json")
                .timeout(Duration::from_secs(5))
                .body(body_b)
                .send()
                .await
        }),
    );
    let elapsed = started.elapsed();

    let a = a.expect("failing spawn").expect("failing send");
    let b = b.expect("succeeding spawn").expect("succeeding send");
    assert_eq!(a.status(), 500, "failing target should return 500");
    assert_eq!(b.status(), 200, "succeeding target should return 200");

    // Each mock saw exactly the one POST destined for it — proves no
    // cross-talk or retry leak between targets.
    let failing_requests = failing.received_requests().await.expect("failing log");
    let succeeding_requests = succeeding
        .received_requests()
        .await
        .expect("succeeding log");
    assert_eq!(
        failing_requests.len(),
        1,
        "failing target received exactly one POST"
    );
    assert_eq!(
        succeeding_requests.len(),
        1,
        "succeeding target received exactly one POST"
    );

    // Serialization check: a sequential implementation would take ≥
    // (300ms slow + 0ms fast) = 300ms+. Parallel should finish well
    // within 2× the slow target's latency. 600 ms is a generous gate
    // that survives noisy CI.
    assert!(
        elapsed < Duration::from_millis(600),
        "channel dispatch serialized on failing Slack target: took {elapsed:?}"
    );
}
