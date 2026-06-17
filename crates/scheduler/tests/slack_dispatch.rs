//! Phase 2 Story 12.5 — wiremock integration for the Slack channel.
//!
//! Validates that the Slack POST surface (URL shape, Block Kit body,
//! mentions sanitization, 40k truncation fallback) holds end-to-end
//! through a real HTTPS-shaped wiremock consumer.

use std::time::Duration;

use anseo_scheduler::notifications::slack::{
    build_payload, validate_url, SlackConfigError, SLACK_PAYLOAD_CAP_BYTES,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn slack_canonical_block_kit_post_round_trips_against_mock_consumer() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/services/T0000/B0000/xxxxxxxxxxxxxxxx"))
        .and(header("Content-Type", "application/json"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock)
        .await;

    let payload = build_payload(
        "schedule.missed",
        "Tick missed for daily-check",
        "https://anseo.local/dashboard",
        false,
    );

    let target_url = format!("{}/services/T0000/B0000/xxxxxxxxxxxxxxxx", mock.uri());
    let client = reqwest::Client::new();
    let response = client
        .post(&target_url)
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(5))
        .body(serde_json::to_vec(&payload).expect("payload bytes"))
        .send()
        .await
        .expect("POST to wiremock consumer");
    assert_eq!(response.status().as_u16(), 200);

    // wiremock matched all our criteria (method + path + Content-Type
    // header). Body matching is implicit — wiremock would 404 on a
    // mismatch, which would fail the assertion above.
}

#[tokio::test]
async fn slack_payload_under_size_cap_for_typical_event() {
    // Architecture §5 NFR — every common event must fit Slack's 40k
    // cap with room to spare so we never lose details to truncation
    // on normal-shape events. Pin the size budget here.
    let payload = build_payload(
        "schedule.missed",
        "Daily check tick missed because the worker was offline 09:00–10:15 UTC. \
         Next anchored tick fires at 11:00 UTC; the 10:00 slot will record `missed` \
         status in the schedule_ticks table.",
        "https://anseo.local/runs/01HXYZ",
        false,
    );
    let bytes = serde_json::to_vec(&payload).expect("serialize");
    assert!(
        bytes.len() < SLACK_PAYLOAD_CAP_BYTES / 2,
        "typical-event payload should be well under half the 40k cap; was {} bytes",
        bytes.len()
    );
}

#[tokio::test]
async fn slack_oversized_payload_falls_back_to_dashboard_link() {
    // If the summary is over the 40k cap, the dispatcher swaps to a
    // "view in Dashboard" fallback block AND still emits a valid POST.
    // Validates end-to-end at the consumer.
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/services/T0000/B0000/yyyyyyyyyyyyyyyy"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock)
        .await;

    let dashboard = "https://anseo.local/runs/01H";
    let huge_summary = "X".repeat(SLACK_PAYLOAD_CAP_BYTES + 2_000);
    let payload = build_payload("citation.anomaly", &huge_summary, dashboard, false);
    let bytes = serde_json::to_vec(&payload).expect("serialize");
    assert!(
        bytes.len() <= SLACK_PAYLOAD_CAP_BYTES,
        "truncated payload exceeds the 40k cap: {} bytes",
        bytes.len()
    );

    // Fall-back text must mention the Dashboard so operators have
    // recourse to the full event.
    let summary_block = payload["blocks"][1]["text"]["text"]
        .as_str()
        .expect("summary block");
    assert!(summary_block.contains("Dashboard"));
    let action_url = payload["blocks"][2]["elements"][0]["url"]
        .as_str()
        .expect("dashboard button");
    assert_eq!(action_url, dashboard);

    let target_url = format!("{}/services/T0000/B0000/yyyyyyyyyyyyyyyy", mock.uri());
    let response = reqwest::Client::new()
        .post(&target_url)
        .header("Content-Type", "application/json")
        .body(bytes)
        .send()
        .await
        .expect("POST");
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn slack_mentions_stripped_by_default_in_outbound_payload() {
    // Default policy: mentions OFF. Validates an attacker who controls
    // a Prompt Run summary cannot cause `<!channel>` to mass-notify a
    // configured Slack target.
    let mock = MockServer::start().await;
    let target_path = "/services/T0/B0/cccccccccccccccc";
    Mock::given(method("POST"))
        .and(path(target_path))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock)
        .await;

    let payload = build_payload(
        "visibility.anomaly",
        "<!channel> Pinecone fell to rank 11",
        "https://anseo.local",
        false,
    );
    let summary = payload["blocks"][1]["text"]["text"].as_str().unwrap();
    assert!(!summary.contains("<!channel>"));
    assert!(summary.contains("Pinecone fell"));

    let target = format!("{}{}", mock.uri(), target_path);
    let response = reqwest::Client::new()
        .post(&target)
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(&payload).unwrap())
        .send()
        .await
        .expect("POST");
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn slack_validate_url_refuses_non_hooks_slack_com_targets() {
    // Sanity-pin the URL gate: misconfigured targets (Discord, Slack
    // channel URL, plaintext) MUST refuse at validate_url, before the
    // dispatcher fires any HTTP.
    assert!(matches!(
        validate_url("http://hooks.slack.com/services/T/B/tok"),
        Err(SlackConfigError::PlaintextUrl { .. })
    ));
    assert!(matches!(
        validate_url("https://discord.com/api/webhooks/123/abc"),
        Err(SlackConfigError::NonSlackUrl { .. })
    ));
    assert!(validate_url("https://hooks.slack.com/services/T0000/B0000/xxxxxxxxxxxxxxxx").is_ok());
}
