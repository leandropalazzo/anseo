//! Notification channels for Phase 2 Story 12.5 (FR-36).
//!
//! Notifications wrap webhook delivery in operator-friendly channels:
//! Slack Block Kit posts and SMTP emails. Each channel is its own
//! sub-module so the dispatch surface stays narrow per channel:
//!
//! - [`slack`] — HTTPS POST to a Slack incoming-webhook URL with a
//!   Block Kit payload. No HMAC signing (Slack's URL is the auth).
//! - SMTP — TLS-required SMTP send. Plaintext SMTP is refused at
//!   config-parse time per architecture §5 NFR. Lands in a follow-up
//!   round because it pulls in the `lettre` crate + TLS feature
//!   configuration.

pub mod slack;
pub mod smtp;
