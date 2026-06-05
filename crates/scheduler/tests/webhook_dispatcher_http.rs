//! P0-127 / P1-128 surface — `deliver_one` against a real HTTP server.
//!
//! Uses `wiremock` to spin up an ephemeral consumer that records the
//! request shape (header, body) and returns canned status codes. Covers
//! the three classification buckets end-to-end, plus the signature
//! round-trip (sign on the dispatcher side → verify on the consumer
//! side via the same `signer::verify`).
//!
//! These do not need a live database — they exercise the HTTP edge of
//! the dispatcher only.

use std::time::Duration;

use anseo_scheduler::webhooks::dispatcher::{deliver_one, DeliveryOutcome};
use anseo_scheduler::webhooks::signer::{verify, SIGNATURE_HEADER};
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const SECRET: &[u8] = b"webhook-test-secret-bytes-32-len";

#[tokio::test]
async fn delivers_on_2xx_and_records_status() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .and(header_exists(SIGNATURE_HEADER))
        .respond_with(ResponseTemplate::new(204))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let (outcome, _snippet) = deliver_one(
        &client,
        &format!("{}/hook", mock.uri()),
        SECRET,
        br#"{"event_kind":"prompt_run.completed"}"#,
        1_700_000_000,
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(outcome, DeliveryOutcome::Delivered { status: 204 });
}

#[tokio::test]
async fn retries_on_5xx_with_response_status() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(503).set_body_string("upstream down"))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let (outcome, snippet) = deliver_one(
        &client,
        &format!("{}/hook", mock.uri()),
        SECRET,
        b"{}",
        1_700_000_000,
        Duration::from_secs(5),
    )
    .await;
    match outcome {
        DeliveryOutcome::RetryableFailure { status, reason } => {
            assert_eq!(status, Some(503));
            assert!(reason.contains("503"));
        }
        other => panic!("expected RetryableFailure(503), got {other:?}"),
    }
    assert!(snippet.contains("upstream down"));
}

#[tokio::test]
async fn retries_on_429_rate_limit() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let (outcome, _) = deliver_one(
        &client,
        &format!("{}/hook", mock.uri()),
        SECRET,
        b"{}",
        1_700_000_000,
        Duration::from_secs(5),
    )
    .await;
    assert!(
        outcome.is_retryable(),
        "429 must be retryable, got {outcome:?}"
    );
}

#[tokio::test]
async fn permanent_failure_on_400() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad shape"))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let (outcome, _) = deliver_one(
        &client,
        &format!("{}/hook", mock.uri()),
        SECRET,
        b"{}",
        1_700_000_000,
        Duration::from_secs(5),
    )
    .await;
    match outcome {
        DeliveryOutcome::PermanentFailure { status, .. } => assert_eq!(status, 400),
        other => panic!("expected PermanentFailure(400), got {other:?}"),
    }
}

#[tokio::test]
async fn permanent_failure_on_404_gone_consumer() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let (outcome, _) = deliver_one(
        &client,
        &format!("{}/hook", mock.uri()),
        SECRET,
        b"{}",
        1_700_000_000,
        Duration::from_secs(5),
    )
    .await;
    assert!(matches!(
        outcome,
        DeliveryOutcome::PermanentFailure { status: 404, .. }
    ));
}

#[tokio::test]
async fn timeout_is_retryable_with_no_status() {
    // Server delays response beyond the call's timeout. Outcome should be
    // RetryableFailure with status: None (no HTTP layer status to report).
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(2_000)))
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let (outcome, _) = deliver_one(
        &client,
        &format!("{}/hook", mock.uri()),
        SECRET,
        b"{}",
        1_700_000_000,
        Duration::from_millis(200),
    )
    .await;
    match outcome {
        DeliveryOutcome::RetryableFailure { status, .. } => assert!(status.is_none()),
        other => panic!("expected RetryableFailure(timeout), got {other:?}"),
    }
}

#[tokio::test]
async fn signature_verifies_on_consumer_side_round_trip() {
    // Capture the actual headers + body the dispatcher sent, then run them
    // through the verify path with the same secret. This is the round-trip
    // P0-127 test: prove the consumer can authenticate what we send.
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Captured {
        signature: Option<String>,
        body: Option<Vec<u8>>,
    }
    let captured = Arc::new(Mutex::new(Captured::default()));

    let mock = MockServer::start().await;
    let cap_clone = Arc::clone(&captured);
    Mock::given(method("POST"))
        .and(path("/hook"))
        .respond_with(move |req: &Request| {
            let mut cap = cap_clone.lock().expect("captured mutex");
            cap.signature = req
                .headers
                .get(SIGNATURE_HEADER)
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            cap.body = Some(req.body.clone());
            ResponseTemplate::new(200)
        })
        .mount(&mock)
        .await;

    let client = reqwest::Client::new();
    let body = br#"{"event_kind":"prompt_run.completed"}"#;
    let ts = 1_700_000_000;
    let (outcome, _) = deliver_one(
        &client,
        &format!("{}/hook", mock.uri()),
        SECRET,
        body,
        ts,
        Duration::from_secs(5),
    )
    .await;
    assert!(matches!(outcome, DeliveryOutcome::Delivered { .. }));

    let cap = captured.lock().expect("captured mutex");
    let sig = cap
        .signature
        .as_deref()
        .expect("dispatcher must send X-Anseo-Signature");
    let received_body = cap.body.as_ref().expect("dispatcher must send body");
    assert_eq!(received_body, body);

    // Run the same signer::verify the consumer reference impl uses. A
    // pass here proves end-to-end that the dispatcher's wire format and
    // verify path are mutually compatible.
    verify(SECRET, received_body, Some(sig), ts + 30, 300)
        .expect("signature must verify against the same secret + body");
}
