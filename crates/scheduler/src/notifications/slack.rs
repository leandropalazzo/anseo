//! Slack channel for Phase 2 Story 12.5 (FR-36).
//!
//! Pure-logic substrate: URL validation, mentions sanitization, Block
//! Kit payload assembly per event_kind, and 40k truncation with a
//! "view in Dashboard" fallback. The HTTPS POST to Slack lives in a
//! follow-up [`dispatch`] module that uses the same reqwest client the
//! webhook dispatcher does.
//!
//! Mentions policy (architecture §5 / NFR): mentions opt-in default-OFF.
//! An operator who wants `@channel` or `<@U…>` notifications must
//! explicitly enable them per target. Default behaviour strips any
//! Slack mention markup from outbound payloads so an attacker who
//! controls a Prompt Run output cannot mass-notify a Slack channel.

use serde_json::{json, Value};

/// Slack incoming-webhook payload size cap (Slack documents ~40k).
pub const SLACK_PAYLOAD_CAP_BYTES: usize = 40_000;

/// Margin reserved for the truncation banner so we never produce a
/// payload that's at the cap minus the banner.
const TRUNCATION_BANNER_BUDGET_BYTES: usize = 512;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SlackConfigError {
    #[error("Slack URL `{url}` is not an `https://hooks.slack.com/services/…` incoming-webhook")]
    NonSlackUrl { url: String },
    #[error("Slack URL must be https://; got `{url}`")]
    PlaintextUrl { url: String },
}

/// Validate the URL operators register against. Slack incoming-webhook
/// URLs have the canonical shape:
///
/// `https://hooks.slack.com/services/T<team>/B<bot>/<random-token>`
///
/// We don't enforce the deeper shape (Slack's path schema is theirs to
/// change). The substring + https checks are enough to reject the most
/// common config errors: paste of a Discord webhook, paste of a Slack
/// channel URL (not the webhook), or accidental http://.
pub fn validate_url(url: &str) -> Result<(), SlackConfigError> {
    if !url.starts_with("https://") {
        return Err(SlackConfigError::PlaintextUrl {
            url: url.to_string(),
        });
    }
    if !url.starts_with("https://hooks.slack.com/services/") {
        return Err(SlackConfigError::NonSlackUrl {
            url: url.to_string(),
        });
    }
    Ok(())
}

/// Strip Slack mention markup from arbitrary text. Returns the cleaned
/// string. The patterns stripped:
/// - `<@U…>` and `<@W…>` — user mentions
/// - `<!channel>`, `<!here>`, `<!everyone>` — broadcast mentions
/// - `<!subteam^…>` — user-group mentions
///
/// Plain text containing `@` or `#` is untouched — only the Slack-
/// specific `<…>` markup form notifies.
pub fn strip_mentions(text: &str) -> String {
    // Walk the string; whenever we hit `<` followed by an `@` or `!`,
    // skip until the matching `>` (or end-of-string). Simple state
    // machine — no regex dep.
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'@' || next == b'!' {
                // Skip up to and including the matching '>' if any.
                if let Some(end_offset) = bytes[i + 1..].iter().position(|&b| b == b'>') {
                    i += end_offset + 2;
                    continue;
                } else {
                    // No closing '>' — bail out (treat as literal).
                    break;
                }
            }
        }
        let ch_len = std::str::from_utf8(&bytes[i..])
            .map(|s| s.chars().next().map(|c| c.len_utf8()).unwrap_or(1))
            .unwrap_or(1);
        out.push_str(std::str::from_utf8(&bytes[i..i + ch_len]).unwrap_or(""));
        i += ch_len;
    }
    out
}

/// Build a Slack Block Kit payload for one event. `dashboard_url` is the
/// link the truncation banner falls back on if the assembled payload
/// would exceed the 40k cap.
///
/// `mentions_enabled = false` is the architecture-mandated default —
/// stripping mention markup from `summary`. Operators flip it per
/// notification target.
pub fn build_payload(
    event_kind: &str,
    summary: &str,
    dashboard_url: &str,
    mentions_enabled: bool,
) -> Value {
    let summary_clean = if mentions_enabled {
        summary.to_string()
    } else {
        strip_mentions(summary)
    };

    let header_text = match event_kind {
        "prompt_run.completed" => "Prompt run completed",
        "visibility.regression" => "Visibility regression",
        "schedule.missed" => "Scheduled run missed",
        "visibility.anomaly" => "Visibility anomaly",
        "citation.anomaly" => "Citation anomaly",
        other => other,
    };

    let payload = json!({
        "blocks": [
            {
                "type": "header",
                "text": {
                    "type": "plain_text",
                    "text": format!("OpenGEO: {header_text}"),
                    "emoji": true
                }
            },
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": summary_clean
                }
            },
            {
                "type": "actions",
                "elements": [
                    {
                        "type": "button",
                        "text": {
                            "type": "plain_text",
                            "text": "View in Dashboard"
                        },
                        "url": dashboard_url
                    }
                ]
            }
        ]
    });

    let assembled_bytes = serde_json::to_vec(&payload).expect("Value→bytes never fails");
    if assembled_bytes.len() <= SLACK_PAYLOAD_CAP_BYTES - TRUNCATION_BANNER_BUDGET_BYTES {
        return payload;
    }

    // Over the cap — drop the body, keep the header + action button,
    // and post a "truncated; view in Dashboard" line.
    json!({
        "blocks": [
            {
                "type": "header",
                "text": {
                    "type": "plain_text",
                    "text": format!("OpenGEO: {header_text}"),
                    "emoji": true
                }
            },
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": "Event details exceed Slack's 40 KB limit. View the full event in the OpenGEO Dashboard."
                }
            },
            {
                "type": "actions",
                "elements": [
                    {
                        "type": "button",
                        "text": {
                            "type": "plain_text",
                            "text": "View in Dashboard"
                        },
                        "url": dashboard_url
                    }
                ]
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_URL: &str = "https://hooks.slack.com/services/T0000/B0000/xxxxxxxxxxxxxxxx";
    const DASHBOARD: &str = "https://opengeo.local/runs/01H";

    #[test]
    fn validate_url_accepts_canonical_incoming_webhook() {
        assert!(validate_url(FIXTURE_URL).is_ok());
    }

    #[test]
    fn validate_url_rejects_plaintext_http() {
        let err = validate_url("http://hooks.slack.com/services/T/B/tok").unwrap_err();
        assert!(matches!(err, SlackConfigError::PlaintextUrl { .. }));
    }

    #[test]
    fn validate_url_rejects_non_slack_host() {
        let err = validate_url("https://discord.com/api/webhooks/123/abc").unwrap_err();
        assert!(matches!(err, SlackConfigError::NonSlackUrl { .. }));
    }

    #[test]
    fn validate_url_rejects_slack_channel_url_not_webhook() {
        let err = validate_url("https://example.slack.com/archives/C0000").unwrap_err();
        assert!(matches!(err, SlackConfigError::NonSlackUrl { .. }));
    }

    #[test]
    fn strip_mentions_removes_user_mention_markup() {
        let cleaned = strip_mentions("ping <@U12345> please review");
        assert_eq!(cleaned, "ping  please review");
    }

    #[test]
    fn strip_mentions_removes_broadcast_markers() {
        let cleaned = strip_mentions("<!channel> incident now");
        assert_eq!(cleaned, " incident now");
        let cleaned = strip_mentions("<!here>");
        assert_eq!(cleaned, "");
        let cleaned = strip_mentions("<!everyone> alert");
        assert_eq!(cleaned, " alert");
    }

    #[test]
    fn strip_mentions_removes_subteam_markers() {
        let cleaned = strip_mentions("<!subteam^S12345|ops-on-call> please page");
        assert_eq!(cleaned, " please page");
    }

    #[test]
    fn strip_mentions_leaves_literal_at_alone() {
        // A literal "@oncall" is NOT a Slack mention — only the
        // `<…>` form notifies. We leave plain text untouched.
        let text = "email oncall@example.com or #ops in chat";
        assert_eq!(strip_mentions(text), text);
    }

    #[test]
    fn strip_mentions_handles_unclosed_markup_gracefully() {
        // Truncated markup without `>` — don't infinite-loop, just bail
        // at the first unclosed `<@`.
        let cleaned = strip_mentions("safe text <@U-truncated");
        assert_eq!(cleaned, "safe text ");
    }

    #[test]
    fn build_payload_strips_mentions_by_default() {
        let payload =
            build_payload("schedule.missed", "<!channel> tick missed", DASHBOARD, false);
        let blocks = payload["blocks"].as_array().unwrap();
        let summary = blocks[1]["text"]["text"].as_str().unwrap();
        assert!(!summary.contains("<!channel>"));
        assert!(summary.contains("tick missed"));
    }

    #[test]
    fn build_payload_preserves_mentions_when_enabled() {
        let payload =
            build_payload("schedule.missed", "<!channel> tick missed", DASHBOARD, true);
        let blocks = payload["blocks"].as_array().unwrap();
        let summary = blocks[1]["text"]["text"].as_str().unwrap();
        assert!(summary.contains("<!channel>"));
    }

    #[test]
    fn build_payload_maps_each_event_kind_to_a_header() {
        for (kind, expected) in [
            ("prompt_run.completed", "Prompt run completed"),
            ("visibility.regression", "Visibility regression"),
            ("schedule.missed", "Scheduled run missed"),
            ("visibility.anomaly", "Visibility anomaly"),
            ("citation.anomaly", "Citation anomaly"),
        ] {
            let payload = build_payload(kind, "summary", DASHBOARD, false);
            let header = payload["blocks"][0]["text"]["text"].as_str().unwrap();
            assert!(
                header.contains(expected),
                "kind {kind} should map to header containing `{expected}`, got `{header}`"
            );
        }
    }

    #[test]
    fn build_payload_includes_dashboard_button_url() {
        let payload = build_payload("schedule.missed", "msg", DASHBOARD, false);
        let url = payload["blocks"][2]["elements"][0]["url"].as_str().unwrap();
        assert_eq!(url, DASHBOARD);
    }

    #[test]
    fn build_payload_truncates_when_oversize_with_fallback_text() {
        // Build a summary string large enough to push the assembled
        // payload over the cap.
        let huge_summary = "X".repeat(SLACK_PAYLOAD_CAP_BYTES + 1_000);
        let payload = build_payload("schedule.missed", &huge_summary, DASHBOARD, false);
        let bytes = serde_json::to_vec(&payload).unwrap();
        assert!(
            bytes.len() <= SLACK_PAYLOAD_CAP_BYTES,
            "truncated payload {} bytes must fit Slack's 40k cap",
            bytes.len()
        );
        // The fallback text must point operators at the dashboard.
        let summary = payload["blocks"][1]["text"]["text"].as_str().unwrap();
        assert!(summary.contains("Dashboard"));
        // And the button URL must survive truncation.
        let url = payload["blocks"][2]["elements"][0]["url"].as_str().unwrap();
        assert_eq!(url, DASHBOARD);
    }

    #[test]
    fn payload_cap_constant_matches_slack_docs() {
        // Slack documents the limit as 40 KB; pin so a future tweak
        // surfaces with an obvious diff.
        assert_eq!(SLACK_PAYLOAD_CAP_BYTES, 40_000);
    }
}
