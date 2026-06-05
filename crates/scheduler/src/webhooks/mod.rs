//! Webhook delivery substrate for Phase 2 Story 12.4 (FR-35, ARCH-9, C-7).
//!
//! Two pure-logic modules — the HTTP dispatcher, persistence, and CLI
//! wiring land in follow-up rounds:
//!
//! - [`signer`] — HMAC-SHA256 signing + timing-safe verification of the
//!   `X-Anseo-Signature: v1=t={ts},s={hex}` wire shape (architecture §5.2).
//! - [`retry`] — the canonical retry ladder (1s, 30s, 5min, 1h, 6h),
//!   auto-disable after 5 permanent failures (architecture §5.4).

pub mod dispatcher;
pub mod fanout;
pub mod poller;
pub mod retry;
pub mod signer;
pub mod tick;
