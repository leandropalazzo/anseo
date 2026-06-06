//! Send orchestration — the compliance gate every send passes through.
//!
//! The orchestrator sits between the templates and the transport. For a
//! **marketing** send it enforces, in order:
//!   1. suppression list (AC-2/AC-3 unsubscribe honoured),
//!   2. the recipient's granular preferences (rank-change toggle,
//!      all-marketing-off) (AC-3),
//!   3. the EU consent gate — `marketing_consent` must be `true` for an
//!      EU-resident recipient; soft-opt-in does NOT apply (AC-4).
//!
//! For a **transactional** send it applies the magic-link idempotency guard
//! (AC-5): a magic-link email is not retried more than once within the token's
//! validity window.
//!
//! Every attempt — sent, failed, suppressed, or consent-blocked — is written
//! to the audit log with the recipient hashed and the email type / error
//! (AC-5).
//!
//! The pure decision functions ([`marketing_decision`]) are unit-tested without
//! a database; [`Dispatcher::send_marketing`] / [`Dispatcher::send_transactional`]
//! wire them to the repo + transport.

use crate::repo::{CommsRepo, SendOutcome, Subscription};
use crate::template::Message;
use crate::transport::Transport;

/// The outcome of the pure marketing-gate decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingDecision {
    /// All gates passed — proceed to send.
    Send,
    /// Recipient is on the suppression list.
    SuppressedByList,
    /// Recipient turned this category (or all marketing) off.
    DisabledByPreference,
    /// EU resident without explicit `marketing_consent` (AC-4).
    BlockedByEuConsent,
}

impl MarketingDecision {
    /// Map a non-`Send` decision to the audit outcome it should log.
    pub fn audit_outcome(self) -> Option<SendOutcome> {
        match self {
            MarketingDecision::Send => None,
            MarketingDecision::SuppressedByList => Some(SendOutcome::Suppressed),
            MarketingDecision::DisabledByPreference => Some(SendOutcome::Suppressed),
            MarketingDecision::BlockedByEuConsent => Some(SendOutcome::ConsentBlocked),
        }
    }
}

/// Marketing category the dispatch concerns — drives which preference toggle
/// gates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketingCategory {
    RankChange,
    Digest,
}

/// PURE decision: should this marketing email go out?
///
/// `suppressed` is the result of [`CommsRepo::is_marketing_suppressed`]. The
/// rest comes from the recipient's [`Subscription`]. This function has no I/O so
/// it can be exhaustively unit-tested (AC-2, AC-3, AC-4).
pub fn marketing_decision(
    suppressed: bool,
    sub: &Subscription,
    category: MarketingCategory,
) -> MarketingDecision {
    // 1. Hard suppression wins over everything.
    if suppressed {
        return MarketingDecision::SuppressedByList;
    }
    // 2. Master kill-switch + per-category preference.
    if sub.all_marketing_off {
        return MarketingDecision::DisabledByPreference;
    }
    let category_enabled = match category {
        MarketingCategory::RankChange => sub.rank_change_enabled,
        MarketingCategory::Digest => sub.digest_frequency != "off",
    };
    if !category_enabled {
        return MarketingDecision::DisabledByPreference;
    }
    // 3. EU consent gate (AC-4): EU resident requires explicit opt-in.
    //    Soft-opt-in does NOT apply.
    if sub.is_eu_resident && !sub.marketing_consent {
        return MarketingDecision::BlockedByEuConsent;
    }
    MarketingDecision::Send
}

/// Result of a dispatch attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchResult {
    Sent,
    /// Skipped before reaching the wire; carries the reason.
    Skipped(MarketingDecision),
    /// De-duplicated: a magic-link with this key already went out (AC-5).
    AlreadySent,
    /// Transport failed; the failure was audited.
    Failed(String),
}

/// Orchestrator bound to a repo + transport.
pub struct Dispatcher<'a, T: Transport> {
    repo: CommsRepo<'a>,
    transport: &'a T,
}

impl<'a, T: Transport> Dispatcher<'a, T> {
    pub fn new(pool: &'a sqlx::PgPool, transport: &'a T) -> Self {
        Self {
            repo: CommsRepo::new(pool),
            transport,
        }
    }

    /// Send a marketing email through the full compliance gate.
    ///
    /// `recipient_hash` must be [`crate::recipient_hash`] of the address.
    /// `email_type` is the audit label (e.g. `"rank_change"`).
    pub async fn send_marketing(
        &self,
        recipient_hash: &str,
        category: MarketingCategory,
        email_type: &str,
        message: &Message,
    ) -> Result<DispatchResult, sqlx::Error> {
        let stream = message.stream.as_str();

        // Materialise the recipient's preference state. No subscription row =
        // no opt-in = do not send.
        let sub = match self.repo.get_subscription(recipient_hash).await? {
            Some(s) => s,
            None => {
                self.repo
                    .log_send(
                        recipient_hash,
                        stream,
                        email_type,
                        SendOutcome::Suppressed,
                        Some("no subscription / not opted in"),
                        None,
                    )
                    .await?;
                return Ok(DispatchResult::Skipped(
                    MarketingDecision::DisabledByPreference,
                ));
            }
        };

        let suppressed = self.repo.is_marketing_suppressed(recipient_hash).await?;
        let decision = marketing_decision(suppressed, &sub, category);

        if decision != MarketingDecision::Send {
            let outcome = decision.audit_outcome().unwrap_or(SendOutcome::Suppressed);
            self.repo
                .log_send(recipient_hash, stream, email_type, outcome, None, None)
                .await?;
            return Ok(DispatchResult::Skipped(decision));
        }

        match self.transport.send(message).await {
            Ok(()) => {
                self.repo
                    .log_send(
                        recipient_hash,
                        stream,
                        email_type,
                        SendOutcome::Sent,
                        None,
                        None,
                    )
                    .await?;
                Ok(DispatchResult::Sent)
            }
            Err(e) => {
                let err = e.to_string();
                self.repo
                    .log_send(
                        recipient_hash,
                        stream,
                        email_type,
                        SendOutcome::Failed,
                        Some(&err),
                        None,
                    )
                    .await?;
                Ok(DispatchResult::Failed(err))
            }
        }
    }

    /// Send a transactional email. Transactional mail is single-purpose and
    /// always allowed (it carries the thing the recipient asked for) — but a
    /// magic-link send is de-duplicated by `dedup_key` so it is not retried more
    /// than once within the token's validity window (AC-5).
    pub async fn send_transactional(
        &self,
        recipient_hash: &str,
        email_type: &str,
        dedup_key: Option<&str>,
        message: &Message,
    ) -> Result<DispatchResult, sqlx::Error> {
        let stream = message.stream.as_str();

        // Magic-link path: reserve the dedup key ATOMICALLY before sending so
        // two concurrent requests can never both reach the transport (AC-5).
        // The reservation row already records the `sent` outcome; on send
        // failure we downgrade it so a retry within the window can re-reserve.
        if let Some(key) = dedup_key {
            if !self
                .repo
                .reserve_dedup(recipient_hash, stream, email_type, key)
                .await?
            {
                return Ok(DispatchResult::AlreadySent);
            }
            return match self.transport.send(message).await {
                Ok(()) => Ok(DispatchResult::Sent),
                Err(e) => {
                    let err = e.to_string();
                    self.repo.mark_dedup_failed(key, &err).await?;
                    Ok(DispatchResult::Failed(err))
                }
            };
        }

        // Non-deduplicated transactional path: send, then audit the outcome.
        match self.transport.send(message).await {
            Ok(()) => {
                self.repo
                    .log_send(
                        recipient_hash,
                        stream,
                        email_type,
                        SendOutcome::Sent,
                        None,
                        None,
                    )
                    .await?;
                Ok(DispatchResult::Sent)
            }
            Err(e) => {
                let err = e.to_string();
                self.repo
                    .log_send(
                        recipient_hash,
                        stream,
                        email_type,
                        SendOutcome::Failed,
                        Some(&err),
                        None,
                    )
                    .await?;
                Ok(DispatchResult::Failed(err))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(eu: bool, consent: bool) -> Subscription {
        Subscription {
            recipient_hash: "rh".into(),
            email: "o@acme.com".into(),
            rank_change_enabled: true,
            digest_frequency: "weekly".into(),
            all_marketing_off: false,
            marketing_consent: consent,
            is_eu_resident: eu,
        }
    }

    #[test]
    fn opted_in_non_eu_recipient_gets_marketing() {
        let s = sub(false, false);
        assert_eq!(
            marketing_decision(false, &s, MarketingCategory::RankChange),
            MarketingDecision::Send
        );
    }

    #[test]
    fn suppression_list_blocks_send() {
        let s = sub(false, true);
        assert_eq!(
            marketing_decision(true, &s, MarketingCategory::RankChange),
            MarketingDecision::SuppressedByList
        );
    }

    #[test]
    fn all_marketing_off_blocks_send() {
        let mut s = sub(false, true);
        s.all_marketing_off = true;
        assert_eq!(
            marketing_decision(false, &s, MarketingCategory::RankChange),
            MarketingDecision::DisabledByPreference
        );
    }

    #[test]
    fn category_toggle_off_blocks_only_that_category() {
        let mut s = sub(false, true);
        s.rank_change_enabled = false;
        assert_eq!(
            marketing_decision(false, &s, MarketingCategory::RankChange),
            MarketingDecision::DisabledByPreference
        );
        // Digest still allowed (weekly != off).
        assert_eq!(
            marketing_decision(false, &s, MarketingCategory::Digest),
            MarketingDecision::Send
        );
    }

    #[test]
    fn digest_off_blocks_digest() {
        let mut s = sub(false, true);
        s.digest_frequency = "off".into();
        assert_eq!(
            marketing_decision(false, &s, MarketingCategory::Digest),
            MarketingDecision::DisabledByPreference
        );
    }

    #[test]
    fn eu_resident_without_consent_is_blocked() {
        // AC-4: marketing_consent = false → not sent. Soft-opt-in N/A.
        let s = sub(true, false);
        assert_eq!(
            marketing_decision(false, &s, MarketingCategory::RankChange),
            MarketingDecision::BlockedByEuConsent
        );
    }

    #[test]
    fn eu_resident_with_consent_gets_marketing() {
        let s = sub(true, true);
        assert_eq!(
            marketing_decision(false, &s, MarketingCategory::RankChange),
            MarketingDecision::Send
        );
    }

    #[test]
    fn eu_consent_audit_outcome_is_consent_blocked() {
        assert_eq!(
            MarketingDecision::BlockedByEuConsent.audit_outcome(),
            Some(SendOutcome::ConsentBlocked)
        );
        assert_eq!(MarketingDecision::Send.audit_outcome(), None);
    }
}
