//! HMAC-SHA256 signing + verification for Phase 2 webhook deliveries
//! (architecture §5.2, FR-35, R-201).
//!
//! Wire shape (X-OpenGEO-Signature header):
//!
//! ```text
//! X-OpenGEO-Signature: v1=t={unix-timestamp},s={hex-encoded-hmac-sha256}
//! ```
//!
//! Canonical message the HMAC is computed over:
//!
//! ```text
//! {unix-timestamp}.{request-body-bytes}
//! ```
//!
//! Consumers recompute and compare. Both sides use the same per-webhook
//! shared secret (`secret_ciphertext` in the `webhooks` row, decrypted at
//! send time).
//!
//! Verification is **timing-safe** — byte-by-byte comparison would leak the
//! signature one character at a time to a network attacker who can measure
//! response latency. [`subtle::ConstantTimeEq`] removes that channel.
//!
//! Replay protection is enforced by [`verify`]: timestamps outside the
//! configured window (default 5 minutes) are rejected even if the HMAC is
//! otherwise valid, so a recorded signed message cannot be replayed
//! indefinitely.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Default replay window: signed messages older than 5 minutes are rejected.
pub const DEFAULT_REPLAY_WINDOW_SECONDS: i64 = 300;

/// Wire-stable header name. Stable across Phase 2; a Phase 3 v2 scheme
/// would land alongside (`v2=…`), not replace.
pub const SIGNATURE_HEADER: &str = "X-OpenGEO-Signature";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("signature header is missing or empty")]
    MissingHeader,
    #[error("signature header could not be parsed; expected `v1=t={{ts}},s={{hex}}`")]
    MalformedHeader,
    #[error("signature timestamp `{timestamp}` is outside the replay window (now={now}, window={window}s)")]
    Stale {
        timestamp: i64,
        now: i64,
        window: i64,
    },
    #[error("signature timestamp `{timestamp}` is in the future (now={now}); clock skew?")]
    FutureTimestamp { timestamp: i64, now: i64 },
    #[error("HMAC mismatch")]
    BadSignature,
}

/// Produce a signature for the given body using the per-webhook secret.
/// Output is the full header value, including the `v1=t=…,s=…` prefix.
///
/// Callers MUST send the wall-clock timestamp they used here as part of
/// the request — the consumer recomputes the HMAC against the timestamp
/// embedded in the header, so any drift between sign-time and send-time
/// is locked in by the caller.
pub fn sign(secret: &[u8], body: &[u8], timestamp_unix: i64) -> String {
    let mut mac = HmacSha256::new_from_slice(secret)
        .expect("HMAC-SHA256 accepts any key length, including empty");
    mac.update(timestamp_unix.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let signature = mac.finalize().into_bytes();
    format!("v1=t={timestamp_unix},s={}", hex_lower(&signature))
}

/// Verify the signature header against the body and secret. Returns
/// `Ok(())` only when:
///
/// 1. The header parses as `v1=t={int},s={hex}`.
/// 2. The timestamp is within `[now - window, now]`. `now` MUST be the
///    consumer's wall clock at receipt; callers passing a stale `now`
///    weaken replay protection.
/// 3. The recomputed HMAC matches the supplied signature in constant time.
pub fn verify(
    secret: &[u8],
    body: &[u8],
    signature_header: Option<&str>,
    now_unix: i64,
    window_seconds: i64,
) -> Result<(), VerifyError> {
    let raw = signature_header
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(VerifyError::MissingHeader)?;

    let (timestamp, expected_sig_hex) = parse(raw)?;

    if timestamp > now_unix {
        return Err(VerifyError::FutureTimestamp {
            timestamp,
            now: now_unix,
        });
    }
    if now_unix - timestamp > window_seconds {
        return Err(VerifyError::Stale {
            timestamp,
            now: now_unix,
            window: window_seconds,
        });
    }

    let mut mac = HmacSha256::new_from_slice(secret)
        .expect("HMAC-SHA256 accepts any key length, including empty");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let actual_sig = mac.finalize().into_bytes();
    let actual_sig_hex = hex_lower(&actual_sig);

    // Constant-time comparison: bool conversion from `subtle::Choice`.
    if actual_sig_hex
        .as_bytes()
        .ct_eq(expected_sig_hex.as_bytes())
        .into()
    {
        Ok(())
    } else {
        Err(VerifyError::BadSignature)
    }
}

/// Parse a header value of the form `v1=t={int},s={hex}` into the
/// timestamp and the signature-hex pair. Lenient on whitespace between
/// the `=` and the value (some proxies normalize).
fn parse(raw: &str) -> Result<(i64, &str), VerifyError> {
    // We pin v1; future schemes get their own prefix. Reject everything
    // else with a clean error so an operator sees the version mismatch.
    let rest = raw
        .strip_prefix("v1=")
        .ok_or(VerifyError::MalformedHeader)?;
    // `t={int},s={hex}` — split on the comma.
    let (t_part, s_part) = rest.split_once(',').ok_or(VerifyError::MalformedHeader)?;
    let ts_str = t_part
        .strip_prefix("t=")
        .ok_or(VerifyError::MalformedHeader)?;
    let sig_hex = s_part
        .strip_prefix("s=")
        .ok_or(VerifyError::MalformedHeader)?;
    let timestamp: i64 = ts_str.parse().map_err(|_| VerifyError::MalformedHeader)?;
    if sig_hex.is_empty() || !sig_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(VerifyError::MalformedHeader);
    }
    Ok((timestamp, sig_hex))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"super-secret-32-bytes-fixed-value-x";

    #[test]
    fn header_constant_matches_arch_5_2() {
        assert_eq!(SIGNATURE_HEADER, "X-OpenGEO-Signature");
    }

    #[test]
    fn sign_returns_versioned_header() {
        let sig = sign(SECRET, b"{}", 1_700_000_000);
        assert!(sig.starts_with("v1=t=1700000000,s="));
        // 64 hex chars for SHA256 output.
        let hex_part = sig.split_once("s=").unwrap().1;
        assert_eq!(hex_part.len(), 64);
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let ts = 1_700_000_000;
        let body = br#"{"event_kind":"prompt_run.completed"}"#;
        let header = sign(SECRET, body, ts);
        let result = verify(SECRET, body, Some(&header), ts + 30, 300);
        assert!(result.is_ok(), "verify failed: {result:?}");
    }

    #[test]
    fn verify_rejects_missing_header() {
        let body = b"{}";
        assert_eq!(
            verify(SECRET, body, None, 1_700_000_000, 300),
            Err(VerifyError::MissingHeader)
        );
        assert_eq!(
            verify(SECRET, body, Some(""), 1_700_000_000, 300),
            Err(VerifyError::MissingHeader)
        );
        assert_eq!(
            verify(SECRET, body, Some("  "), 1_700_000_000, 300),
            Err(VerifyError::MissingHeader)
        );
    }

    #[test]
    fn verify_rejects_wrong_version() {
        let bad = "v2=t=1700000000,s=deadbeef";
        let body = b"{}";
        assert_eq!(
            verify(SECRET, body, Some(bad), 1_700_000_000, 300),
            Err(VerifyError::MalformedHeader)
        );
    }

    #[test]
    fn verify_rejects_malformed_header_parts() {
        let body = b"{}";
        let cases = [
            "v1=missing-timestamp",
            "v1=t=notanumber,s=abc",
            "v1=t=1700000000",                  // no signature half
            "v1=t=1700000000,s=",               // empty signature
            "v1=t=1700000000,s=xyz!notHexHere", // non-hex signature
        ];
        for case in cases {
            let result = verify(SECRET, body, Some(case), 1_700_000_000, 300);
            assert_eq!(
                result,
                Err(VerifyError::MalformedHeader),
                "case `{case}` should be malformed, got {result:?}"
            );
        }
    }

    #[test]
    fn verify_rejects_stale_timestamp() {
        let ts = 1_700_000_000;
        let body = b"{}";
        let header = sign(SECRET, body, ts);
        // now is 6 minutes after sign time; window is 5 minutes.
        let now = ts + 360;
        let result = verify(SECRET, body, Some(&header), now, 300);
        match result {
            Err(VerifyError::Stale {
                timestamp,
                now: returned_now,
                window,
            }) => {
                assert_eq!(timestamp, ts);
                assert_eq!(returned_now, now);
                assert_eq!(window, 300);
            }
            other => panic!("expected Stale, got {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_future_timestamp() {
        let ts = 1_700_000_000;
        let body = b"{}";
        let header = sign(SECRET, body, ts);
        // now is BEFORE sign time — clock skew between producer and
        // consumer, or a forgery attempt.
        let now = ts - 30;
        let result = verify(SECRET, body, Some(&header), now, 300);
        assert!(matches!(result, Err(VerifyError::FutureTimestamp { .. })));
    }

    #[test]
    fn verify_rejects_bad_signature() {
        let ts = 1_700_000_000;
        let body = br#"{"event_kind":"prompt_run.completed"}"#;
        let header = sign(SECRET, body, ts);
        // Flip one hex character at the start; HMAC must mismatch.
        let tampered = header
            .replace("s=", "s=ff")
            .chars()
            .take(header.len())
            .collect::<String>();
        let result = verify(SECRET, body, Some(&tampered), ts + 30, 300);
        assert_eq!(result, Err(VerifyError::BadSignature));
    }

    #[test]
    fn verify_rejects_body_mismatch() {
        let ts = 1_700_000_000;
        let body = br#"{"original":"body"}"#;
        let header = sign(SECRET, body, ts);
        let tampered_body = br#"{"tampered":"body"}"#;
        let result = verify(SECRET, tampered_body, Some(&header), ts + 30, 300);
        assert_eq!(result, Err(VerifyError::BadSignature));
    }

    #[test]
    fn verify_rejects_wrong_secret() {
        let ts = 1_700_000_000;
        let body = b"{}";
        let header = sign(SECRET, body, ts);
        let result = verify(b"different-secret", body, Some(&header), ts + 30, 300);
        assert_eq!(result, Err(VerifyError::BadSignature));
    }

    #[test]
    fn verify_accepts_at_window_boundary() {
        // now == sign_time + window exactly is INSIDE the window
        // (`now_unix - timestamp > window_seconds` is strict-greater).
        let ts = 1_700_000_000;
        let body = b"{}";
        let header = sign(SECRET, body, ts);
        let result = verify(SECRET, body, Some(&header), ts + 300, 300);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_extracts_timestamp_and_hex() {
        let (ts, hex) = parse("v1=t=42,s=deadbeef").unwrap();
        assert_eq!(ts, 42);
        assert_eq!(hex, "deadbeef");
    }

    #[test]
    fn hex_lower_pads_and_lowercases() {
        assert_eq!(hex_lower(&[0x00, 0x0f, 0xab, 0xff]), "000fabff");
    }

    #[test]
    fn default_replay_window_is_5_minutes() {
        assert_eq!(DEFAULT_REPLAY_WINDOW_SECONDS, 300);
    }
}
