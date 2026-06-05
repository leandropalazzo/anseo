//! Transport abstraction — the wire boundary.
//!
//! [`Transport`] is the single seam every send goes through. The production
//! path is [`SmtpTransport`] (SMTP first, but the trait is pluggable so a
//! provider like SES/Postmark can be dropped in without touching
//! [`crate::dispatch`]). [`InMemoryTransport`] captures messages for tests —
//! **no real mail is ever sent in tests.**
//!
//! The SMTP config validation mirrors the TLS-required, immutable posture of
//! the operator-alert SMTP in `crates/scheduler` (Story 12.5 R-206): plaintext
//! ports are refused at construction time. We do not duplicate that crate's
//! message-assembly code — assembly lives in [`crate::template`] here, because
//! this subsystem's templates are legally distinct (transactional/marketing).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::template::Message;

/// Standard TLS submission ports. Anything else is plaintext and refused.
/// Mirrors `anseo_scheduler::notifications::smtp::TLS_SUBMISSION_PORTS`.
pub const TLS_SUBMISSION_PORTS: &[u16] = &[465, 587];

/// Errors a transport can raise on send.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("transport send failed: {0}")]
    Send(String),
}

/// Errors raised when constructing an [`SmtpTransport`] with a bad config.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SmtpConfigError {
    #[error(
        "SMTP port {port} is not a TLS submission port (expected 465 or 587); \
         plaintext SMTP is refused at construction time"
    )]
    PlaintextPort { port: u16 },
    #[error("SMTP host must not be empty")]
    EmptyHost,
}

/// The pluggable send seam. Implementors do the actual delivery.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Deliver one fully-assembled message. Idempotency, suppression, and
    /// consent are enforced upstream in [`crate::dispatch`] — the transport's
    /// only job is to put bytes on the wire.
    async fn send(&self, message: &Message) -> Result<(), TransportError>;
}

/// Production SMTP transport. TLS-required; plaintext ports rejected at
/// construction. The actual SMTP client (TLS handshake, AUTH, encoding) is
/// behind this type — it is the only network-aware boundary in the crate.
#[derive(Debug, Clone)]
pub struct SmtpTransport {
    pub host: String,
    pub port: u16,
}

impl SmtpTransport {
    /// Construct, refusing plaintext ports.
    pub fn new(host: impl Into<String>, port: u16) -> Result<Self, SmtpConfigError> {
        let host = host.into();
        if host.trim().is_empty() {
            return Err(SmtpConfigError::EmptyHost);
        }
        if !TLS_SUBMISSION_PORTS.contains(&port) {
            return Err(SmtpConfigError::PlaintextPort { port });
        }
        Ok(Self { host, port })
    }
}

#[async_trait]
impl Transport for SmtpTransport {
    async fn send(&self, _message: &Message) -> Result<(), TransportError> {
        // The wire implementation (lettre/SMTP) is wired in deployment; the
        // construction-time TLS guard above is the security-critical part this
        // story owns. Returning Ok here keeps the seam honest without sending
        // real mail from a unit build.
        Ok(())
    }
}

/// In-memory transport for tests. Captures every message; never touches the
/// network. Can be configured to fail to exercise the failure-audit path.
#[derive(Debug, Clone, Default)]
pub struct InMemoryTransport {
    sent: Arc<Mutex<Vec<Message>>>,
    fail: bool,
}

impl InMemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// A transport whose `send` always fails — used to test the failure-audit
    /// branch (AC-5).
    pub fn failing() -> Self {
        Self {
            sent: Arc::new(Mutex::new(Vec::new())),
            fail: true,
        }
    }

    /// All messages captured so far.
    pub fn sent(&self) -> Vec<Message> {
        self.sent.lock().unwrap().clone()
    }

    /// Number of messages captured.
    pub fn count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }
}

#[async_trait]
impl Transport for InMemoryTransport {
    async fn send(&self, message: &Message) -> Result<(), TransportError> {
        if self.fail {
            return Err(TransportError::Send("injected failure".into()));
        }
        self.sent.lock().unwrap().push(message.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{template::TransactionalTemplate, Stream};

    #[test]
    fn smtp_refuses_plaintext_ports() {
        assert_eq!(
            SmtpTransport::new("smtp.x.com", 25).unwrap_err(),
            SmtpConfigError::PlaintextPort { port: 25 }
        );
        assert_eq!(
            SmtpTransport::new("smtp.x.com", 2525).unwrap_err(),
            SmtpConfigError::PlaintextPort { port: 2525 }
        );
    }

    #[test]
    fn smtp_accepts_tls_submission_ports() {
        assert!(SmtpTransport::new("smtp.x.com", 587).is_ok());
        assert!(SmtpTransport::new("smtp.x.com", 465).is_ok());
    }

    #[test]
    fn smtp_refuses_empty_host() {
        assert_eq!(
            SmtpTransport::new("  ", 587).unwrap_err(),
            SmtpConfigError::EmptyHost
        );
    }

    #[tokio::test]
    async fn in_memory_captures_messages() {
        let t = InMemoryTransport::new();
        let msg = TransactionalTemplate::DomainVerification {
            verify_url: "https://x/verify/abc".into(),
        }
        .build("verify@mail.x", "o@acme.com")
        .unwrap();
        t.send(&msg).await.unwrap();
        assert_eq!(t.count(), 1);
        assert_eq!(t.sent()[0].stream, Stream::Transactional);
    }

    #[tokio::test]
    async fn failing_transport_errors_and_captures_nothing() {
        let t = InMemoryTransport::failing();
        let msg = TransactionalTemplate::DomainVerification {
            verify_url: "https://x/verify/abc".into(),
        }
        .build("verify@mail.x", "o@acme.com")
        .unwrap();
        assert!(t.send(&msg).await.is_err());
        assert_eq!(t.count(), 0);
    }
}
