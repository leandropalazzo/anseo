//! Communications subsystem — Story 43.7.
//!
//! A net-new email subsystem that cleanly separates **transactional** mail
//! (single-purpose, no promotion — e.g. domain-verification magic links) from
//! **marketing** mail (opt-in, one-click unsubscribe + postal address — e.g.
//! rank-change notifications). The split is a legal requirement (CAN-SPAM /
//! ePrivacy); GDPR Art.21(2) makes the EU marketing opt-out immediate and
//! absolute.
//!
//! This crate is deliberately **separate** from the operator-alert SMTP in
//! `crates/scheduler` (Story 12.5). That channel notifies the operator about
//! their own runs; this one notifies external claimants and benchmark
//! subscribers. Different audiences, different legal regime, different
//! subdomains.
//!
//! # Architecture
//!
//! * [`transport`] — the [`transport::Transport`] trait abstracts the wire.
//!   [`transport::SmtpTransport`] is the production path (SMTP first, but the
//!   trait is pluggable for SES/Postmark later). [`transport::InMemoryTransport`]
//!   captures messages for tests — **no real mail is ever sent in tests**.
//! * [`template`] — [`template::TransactionalTemplate`] /
//!   [`template::MarketingTemplate`] assembly. The transactional builder runs a
//!   [`template::content_policy`] guard that REFUSES promotional content, so an
//!   accidental promo string can never ship on the transactional stream.
//! * [`token`] — opaque HMAC-derived preference-center tokens. No-login access
//!   to the preference center and one-click unsubscribe.
//! * [`dispatch`] — the send orchestrator. Applies the suppression list, the
//!   EU-consent gate, the magic-link idempotency guard, and writes the audit
//!   log, then hands the assembled message to the [`transport::Transport`].
//! * [`repo`] — dynamic-`sqlx` access to the `comms_*` tables.

pub mod dispatch;
pub mod recipient;
pub mod repo;
pub mod template;
pub mod token;
pub mod transport;

pub use recipient::recipient_hash;

/// The two legally-distinct email streams. They map to separate sending
/// subdomains so reputation, SPF/DKIM/DMARC alignment, and suppression can be
/// reasoned about independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stream {
    /// Single-purpose mail (verification, revocation). No promo, always
    /// allowed for the recipient (cannot be unsubscribed — it carries the
    /// thing the recipient asked for).
    Transactional,
    /// Opted-in mail (rank-change, digests). Requires consent + honours
    /// suppression and the EU consent gate.
    Marketing,
}

impl Stream {
    /// Wire/DB string form.
    pub fn as_str(self) -> &'static str {
        match self {
            Stream::Transactional => "transactional",
            Stream::Marketing => "marketing",
        }
    }

    /// The sending subdomain for this stream. Transactional and marketing use
    /// *distinct* subdomains so DMARC alignment and reputation are isolated.
    pub fn subdomain(self, root: &str) -> String {
        match self {
            Stream::Transactional => format!("verify@mail.{root}"),
            Stream::Marketing => format!("news@mail.{root}"),
        }
    }
}
