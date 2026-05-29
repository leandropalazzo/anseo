//! Webhook delivery dispatcher (Story 12.4, FR-35).
//!
//! The dispatcher is two layers:
//!
//! 1. [`classify_response`] — pure function mapping `(status, body)` to
//!    a [`DeliveryOutcome`]. Testable without any IO.
//! 2. [`deliver_one`] — async function that signs the body, fires one
//!    HTTP POST via `reqwest`, and feeds the result through
//!    `classify_response`. Integration-testable via `wiremock`
//!    fixtures (deferred until the orchestration tick lands).
//!
//! The polling tick that pulls `webhook_deliveries` rows, fans out
//! per-(event, webhook) Tokio tasks, and writes the outcome back to the
//! repo is the next layer — it depends on the `webhooks` row accessor
//! (secret retrieval, target URL) which is the follow-up Story 12.4
//! work.

use crate::webhooks::signer::{sign, SIGNATURE_HEADER};
use reqwest::Client;
use std::time::Duration;

/// Snippet length captured into `webhook_deliveries.response_body_snippet`.
/// Bounded so the audit trail of N failed deliveries doesn't store a
/// pathological MB-sized error page from a downstream consumer.
pub const RESPONSE_SNIPPET_BYTES: usize = 1024;

/// What happened on one attempted delivery, in the dispatcher's vocabulary
/// rather than the HTTP layer's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// HTTP 2xx — consumer accepted the event. Repo transition:
    /// `mark_delivered`.
    Delivered { status: u16 },
    /// Retryable failure — transient network error, 5xx, 429, or 408.
    /// Repo transition: `mark_failed_retryable` with the next ladder
    /// step.
    RetryableFailure {
        status: Option<u16>,
        reason: String,
    },
    /// Non-retryable failure — 4xx other than 408/429. The consumer told
    /// us this event will never succeed (bad signature, bad shape,
    /// gone). Repo transition: `mark_dropped` immediately, skipping the
    /// remaining ladder.
    PermanentFailure {
        status: u16,
        reason: String,
    },
}

impl DeliveryOutcome {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RetryableFailure { .. })
    }
}

/// Classify an HTTP response without doing any IO. The split between
/// retryable and permanent failure follows the architecture's stance:
/// 5xx + 408 + 429 are transient (worth re-trying); other 4xx are
/// permanent (the consumer is telling us "don't bother").
///
/// 429 is intentionally retryable so a consumer rate-limit doesn't
/// auto-disable the webhook on a brief overload — the architecture's
/// 5-permanent-failures threshold protects against truly broken
/// targets, not blips.
pub fn classify_response(status: u16, body_snippet: &str) -> DeliveryOutcome {
    if (200..300).contains(&status) {
        return DeliveryOutcome::Delivered { status };
    }
    if status == 408 || status == 429 || (500..600).contains(&status) {
        return DeliveryOutcome::RetryableFailure {
            status: Some(status),
            reason: format!(
                "HTTP {status}: {}",
                truncate_for_audit(body_snippet)
            ),
        };
    }
    DeliveryOutcome::PermanentFailure {
        status,
        reason: format!(
            "HTTP {status}: {}",
            truncate_for_audit(body_snippet)
        ),
    }
}

/// One synchronous delivery attempt over reqwest. Returns the classified
/// outcome plus the snippet the repo should persist.
pub async fn deliver_one(
    client: &Client,
    target_url: &str,
    secret: &[u8],
    body: &[u8],
    timestamp_unix: i64,
    timeout: Duration,
) -> (DeliveryOutcome, String) {
    let signature = sign(secret, body, timestamp_unix);
    let request = client
        .post(target_url)
        .header(SIGNATURE_HEADER, signature)
        .header("Content-Type", "application/json")
        .timeout(timeout)
        .body(body.to_vec());

    match request.send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            let snippet = response
                .text()
                .await
                .unwrap_or_else(|_| String::new());
            let outcome = classify_response(status, &snippet);
            (outcome, truncate_for_audit(&snippet))
        }
        Err(err) => {
            // reqwest doesn't surface a status here — connection refused,
            // DNS failure, TLS handshake error, timeout, etc. All
            // retryable; the audit log carries the error string.
            let reason = err.to_string();
            (
                DeliveryOutcome::RetryableFailure {
                    status: None,
                    reason: reason.clone(),
                },
                truncate_for_audit(&reason),
            )
        }
    }
}

fn truncate_for_audit(s: &str) -> String {
    if s.len() <= RESPONSE_SNIPPET_BYTES {
        return s.to_string();
    }
    // Char-boundary safe: take up to RESPONSE_SNIPPET_BYTES bytes by
    // walking chars. A pathological multi-byte char at the boundary won't
    // slice mid-codepoint.
    let mut out = String::with_capacity(RESPONSE_SNIPPET_BYTES);
    for c in s.chars() {
        if out.len() + c.len_utf8() > RESPONSE_SNIPPET_BYTES {
            break;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivered_for_2xx() {
        for status in [200, 201, 202, 204, 299] {
            assert_eq!(
                classify_response(status, "ok"),
                DeliveryOutcome::Delivered { status },
                "status {status} should classify as Delivered"
            );
        }
    }

    #[test]
    fn retryable_for_5xx() {
        for status in [500, 502, 503, 504, 599] {
            let outcome = classify_response(status, "down");
            assert!(
                outcome.is_retryable(),
                "status {status} should be retryable, got {outcome:?}"
            );
        }
    }

    #[test]
    fn retryable_for_429_and_408() {
        assert!(classify_response(408, "timeout").is_retryable());
        assert!(classify_response(429, "rate limited").is_retryable());
    }

    #[test]
    fn permanent_failure_for_other_4xx() {
        for status in [400, 401, 403, 404, 410, 422] {
            let outcome = classify_response(status, "no");
            assert!(
                matches!(outcome, DeliveryOutcome::PermanentFailure { .. }),
                "status {status} should be permanent, got {outcome:?}"
            );
            assert!(!outcome.is_retryable());
        }
    }

    #[test]
    fn permanent_failure_for_3xx_redirects() {
        // Webhooks don't follow redirects — consumer must publish a
        // stable target URL. A 3xx is treated as a config bug.
        let outcome = classify_response(301, "moved");
        assert!(matches!(outcome, DeliveryOutcome::PermanentFailure { .. }));
    }

    #[test]
    fn snippet_truncated_at_audit_limit() {
        let huge = "X".repeat(5_000);
        let small = truncate_for_audit(&huge);
        assert!(small.len() <= RESPONSE_SNIPPET_BYTES);
        assert_eq!(small.len(), RESPONSE_SNIPPET_BYTES);
    }

    #[test]
    fn snippet_preserves_multibyte_codepoints_at_boundary() {
        // Use a 4-byte UTF-8 character (😀 U+1F600 is 4 bytes).
        let mut s = String::new();
        // Fill almost to the limit then add a multi-byte char that
        // would straddle.
        for _ in 0..(RESPONSE_SNIPPET_BYTES - 2) {
            s.push('A');
        }
        s.push('😀'); // 4 bytes — would straddle if we sliced naively.
        s.push('B');
        let out = truncate_for_audit(&s);
        // The emoji should be dropped (would put us over) and the
        // result should end on a valid UTF-8 boundary.
        assert!(out.is_char_boundary(out.len()));
        assert!(out.len() <= RESPONSE_SNIPPET_BYTES);
    }

    #[test]
    fn classification_records_status_in_reason() {
        let outcome = classify_response(503, "Service Unavailable");
        match outcome {
            DeliveryOutcome::RetryableFailure { status, reason } => {
                assert_eq!(status, Some(503));
                assert!(reason.contains("503"));
                assert!(reason.contains("Service Unavailable"));
            }
            other => panic!("expected RetryableFailure, got {other:?}"),
        }
    }

    #[test]
    fn outcome_helpers_distinguish_terminal_vs_retry() {
        assert!(!DeliveryOutcome::Delivered { status: 200 }.is_retryable());
        assert!(!DeliveryOutcome::PermanentFailure {
            status: 400,
            reason: "x".into()
        }
        .is_retryable());
        assert!(DeliveryOutcome::RetryableFailure {
            status: Some(503),
            reason: "x".into()
        }
        .is_retryable());
    }
}
