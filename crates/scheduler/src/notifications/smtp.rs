//! SMTP channel config + subject/body assembly for Phase 2 Story 12.5
//! (FR-36, R-206).
//!
//! The actual SMTP send (TLS handshake, AUTH PLAIN, message encoding)
//! lives in a follow-up round that pulls in the `lettre` crate. This
//! module owns the security-critical bits that don't need an SMTP
//! client:
//!
//! 1. [`validate_config`] — refuses plaintext at parse time per R-206.
//!    The architectural NFR is "TLS-required, immutable" — operators
//!    cannot toggle it per target.
//! 2. [`build_subject`] / [`build_body`] — RFC 5322-safe assembly of
//!    the per-event-kind subject + plaintext body. Operator-friendly
//!    summaries; the full event JSON lives at the dashboard link.

use serde::{Deserialize, Serialize};

/// Standard TLS-required submission ports. Anything outside this set is
/// rejected as plaintext-by-default.
pub const TLS_SUBMISSION_PORTS: &[u16] = &[
    465, // SMTPS implicit TLS
    587, // submission with STARTTLS (MUST upgrade)
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    /// `from:` address that appears in the recipient's mailbox.
    pub from: String,
    /// Default subject prefix; subject lines are built as
    /// `<prefix> <event-kind summary>`.
    #[serde(default = "default_subject_prefix")]
    pub subject_prefix: String,
}

fn default_subject_prefix() -> String {
    "[OpenGEO]".to_string()
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SmtpConfigError {
    #[error(
        "SMTP port {port} is not a TLS submission port (expected 465 or 587). \
         Plaintext SMTP is refused at parse time per Phase 2 NFR — TLS is \
         immutable per target."
    )]
    PlaintextPort { port: u16 },
    #[error("SMTP `host` field must not be empty")]
    EmptyHost,
    #[error("SMTP `from` field must not be empty")]
    EmptyFrom,
    #[error("SMTP `from` field `{from}` does not contain an `@`")]
    InvalidFromAddress { from: String },
}

/// Validate an SMTP config at parse time. The TLS check is non-toggleable:
/// passing 25 (plaintext SMTP), 2525 (alternate plaintext), or any other
/// non-TLS port yields `PlaintextPort`. This is the load-bearing R-206
/// mitigation — Phase 2 ships TLS-or-nothing.
pub fn validate_config(cfg: &SmtpConfig) -> Result<(), SmtpConfigError> {
    if cfg.host.trim().is_empty() {
        return Err(SmtpConfigError::EmptyHost);
    }
    if cfg.from.trim().is_empty() {
        return Err(SmtpConfigError::EmptyFrom);
    }
    if !cfg.from.contains('@') {
        return Err(SmtpConfigError::InvalidFromAddress {
            from: cfg.from.clone(),
        });
    }
    if !TLS_SUBMISSION_PORTS.contains(&cfg.port) {
        return Err(SmtpConfigError::PlaintextPort { port: cfg.port });
    }
    Ok(())
}

/// Build an RFC 5322-safe subject line for one event. Stays under
/// reasonable subject-length conventions (≤ 78 chars where possible) but
/// does not hard-truncate — mail clients render long subjects fine.
pub fn build_subject(cfg: &SmtpConfig, event_kind: &str, brand_context: &str) -> String {
    let title = match event_kind {
        "prompt_run.completed" => "Prompt run completed",
        "visibility.regression" => "Visibility regression",
        "schedule.missed" => "Scheduled run missed",
        "visibility.anomaly" => "Visibility anomaly",
        "citation.anomaly" => "Citation anomaly",
        other => other,
    };
    let trimmed_context = brand_context.trim();
    if trimmed_context.is_empty() {
        format!("{} {title}", cfg.subject_prefix)
    } else {
        format!("{} {title} — {trimmed_context}", cfg.subject_prefix)
    }
}

/// Build a plaintext body for one event. Includes the event summary,
/// observed-at timestamp, and a "view in Dashboard" link the operator
/// follows for the full JSON. Plaintext-only by design — HTML emails
/// would expand the attack surface (markup injection from upstream
/// Prompt Run content).
pub fn build_body(summary: &str, observed_at_iso: &str, dashboard_url: &str) -> String {
    let mut body = String::new();
    body.push_str("OpenGEO event notification\n\n");
    body.push_str("Observed at: ");
    body.push_str(observed_at_iso);
    body.push('\n');
    body.push_str("Summary    : ");
    body.push_str(summary);
    body.push_str("\n\n");
    body.push_str("View the full event details and history in the OpenGEO Dashboard:\n");
    body.push_str(dashboard_url);
    body.push('\n');
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> SmtpConfig {
        SmtpConfig {
            host: "smtp.example.com".into(),
            port: 587,
            from: "opengeo@example.com".into(),
            subject_prefix: "[OpenGEO]".into(),
        }
    }

    #[test]
    fn validate_accepts_canonical_tls_submission() {
        assert!(validate_config(&valid_config()).is_ok());
    }

    #[test]
    fn validate_accepts_smtps_implicit_tls() {
        let cfg = SmtpConfig {
            port: 465,
            ..valid_config()
        };
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn validate_refuses_plaintext_port_25() {
        let cfg = SmtpConfig {
            port: 25,
            ..valid_config()
        };
        let err = validate_config(&cfg).unwrap_err();
        assert!(matches!(err, SmtpConfigError::PlaintextPort { port: 25 }));
    }

    #[test]
    fn validate_refuses_plaintext_port_2525() {
        let cfg = SmtpConfig {
            port: 2525,
            ..valid_config()
        };
        assert!(matches!(
            validate_config(&cfg),
            Err(SmtpConfigError::PlaintextPort { port: 2525 })
        ));
    }

    #[test]
    fn validate_refuses_empty_host() {
        let cfg = SmtpConfig {
            host: "   ".into(),
            ..valid_config()
        };
        assert_eq!(validate_config(&cfg), Err(SmtpConfigError::EmptyHost));
    }

    #[test]
    fn validate_refuses_from_without_at_sign() {
        let cfg = SmtpConfig {
            from: "not-an-email".into(),
            ..valid_config()
        };
        match validate_config(&cfg).unwrap_err() {
            SmtpConfigError::InvalidFromAddress { from } => assert_eq!(from, "not-an-email"),
            other => panic!("expected InvalidFromAddress, got {other:?}"),
        }
    }

    #[test]
    fn validate_refuses_empty_from() {
        let cfg = SmtpConfig {
            from: " ".into(),
            ..valid_config()
        };
        assert_eq!(validate_config(&cfg), Err(SmtpConfigError::EmptyFrom));
    }

    #[test]
    fn subject_includes_prefix_title_and_context() {
        let cfg = valid_config();
        let subject = build_subject(&cfg, "schedule.missed", "Acme");
        assert!(subject.starts_with("[OpenGEO]"));
        assert!(subject.contains("Scheduled run missed"));
        assert!(subject.contains("Acme"));
    }

    #[test]
    fn subject_omits_context_separator_when_empty() {
        let cfg = valid_config();
        let subject = build_subject(&cfg, "schedule.missed", "   ");
        assert!(subject.starts_with("[OpenGEO]"));
        assert!(!subject.contains(" — "));
    }

    #[test]
    fn subject_handles_unknown_event_kind_gracefully() {
        let cfg = valid_config();
        let subject = build_subject(&cfg, "unknown.future_kind", "Acme");
        assert!(subject.contains("unknown.future_kind"));
    }

    #[test]
    fn body_has_dashboard_link_and_observed_at() {
        let body = build_body(
            "z=3.4 detected on openai",
            "2026-06-15T08:00:43.221Z",
            "https://opengeo.local/runs/01H",
        );
        assert!(body.contains("View the full event details"));
        assert!(body.contains("https://opengeo.local/runs/01H"));
        assert!(body.contains("2026-06-15T08:00:43.221Z"));
        assert!(body.contains("z=3.4 detected on openai"));
    }

    #[test]
    fn body_is_plaintext_no_html_markup() {
        // Architecture rationale: HTML expands attack surface from
        // upstream content. Body must be plaintext-only.
        let body = build_body(
            "<script>alert(1)</script>",
            "2026-06-15T08:00:43.221Z",
            "https://opengeo.local/runs/01H",
        );
        // Body should carry the literal text — no escaping, no
        // wrapping in <html>. Plaintext mail clients render this as-is.
        assert!(body.contains("<script>alert(1)</script>"));
        assert!(!body.starts_with("<html"));
    }

    #[test]
    fn tls_submission_ports_pin_arch_5_intent() {
        // The architecture is "TLS-required, immutable per target".
        // Any tweak should surface here.
        assert_eq!(TLS_SUBMISSION_PORTS, &[465, 587]);
    }
}
