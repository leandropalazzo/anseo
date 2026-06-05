//! Email template assembly with a content-policy firewall between the two
//! streams.
//!
//! The legal heart of this story (AC-1): a **transactional** email carries one
//! purpose and NO promotional content. The [`content_policy`] guard scans
//! transactional bodies/subjects for promo markers and refuses to build the
//! message if any are present — so a promo string can never reach the
//! transactional stream by accident.
//!
//! A **marketing** email (AC-2) must carry, by law: an honest subject, the
//! actual content, a one-click unsubscribe link, a physical postal address,
//! and a preference-center link. [`MarketingTemplate::build`] enforces that all
//! of these are present.

use crate::Stream;

/// A fully-assembled message ready to hand to a [`crate::transport::Transport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub stream: Stream,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Errors raised when a template violates its stream's content policy.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error(
        "transactional email contains promotional content (matched marker {marker:?}); \
         transactional mail must be single-purpose with no promotion (CAN-SPAM)"
    )]
    PromotionalContentInTransactional { marker: String },
    #[error("marketing email is missing its one-click unsubscribe link (CAN-SPAM)")]
    MissingUnsubscribeLink,
    #[error("marketing email is missing a physical postal address (CAN-SPAM)")]
    MissingPostalAddress,
    #[error("marketing email is missing the preference-center link")]
    MissingPreferenceLink,
}

/// Content-policy scanner for the transactional stream.
///
/// Returns `Err` if `subject` or `body` contains any promotional marker. The
/// marker set is intentionally conservative — false positives are cheap (an
/// author rewords) while a false negative is a legal violation.
pub mod content_policy {
    use super::TemplateError;

    /// Substrings that signal promotion. Case-insensitive match.
    pub const PROMO_MARKERS: &[&str] = &[
        "unsubscribe", // promo footer artefact leaking into transactional
        "sale",
        "% off",
        "discount",
        "limited time",
        "upgrade now",
        "special offer",
        "buy now",
        "free trial",
        "newsletter",
        "promo",
        "coupon",
        "act now",
    ];

    /// Scan transactional content. The single legitimate purpose (e.g. a verify
    /// link) is fine; only the promo markers are rejected.
    pub fn assert_no_promotion(subject: &str, body: &str) -> Result<(), TemplateError> {
        let haystack = format!("{subject}\n{body}").to_lowercase();
        for marker in PROMO_MARKERS {
            if haystack.contains(marker) {
                return Err(TemplateError::PromotionalContentInTransactional {
                    marker: (*marker).to_string(),
                });
            }
        }
        Ok(())
    }
}

/// Transactional template kinds. Each is single-purpose.
#[derive(Debug, Clone)]
pub enum TransactionalTemplate {
    /// Domain-verification magic link (43.2).
    DomainVerification { verify_url: String },
    /// Verification revoked / claim removed notice (43.2 revocation path).
    VerificationRevoked { domain: String },
}

impl TransactionalTemplate {
    /// Build the message, enforcing the no-promotion policy.
    ///
    /// `from` must be the transactional subdomain (see [`Stream::subdomain`]).
    pub fn build(&self, from: &str, to: &str) -> Result<Message, TemplateError> {
        let (subject, body) = match self {
            TransactionalTemplate::DomainVerification { verify_url } => (
                "Verify your domain".to_string(),
                format!(
                    "You requested to verify ownership of your domain on the Anseo benchmark.\n\n\
                     Open this link to complete verification:\n{verify_url}\n\n\
                     If you did not request this, you can ignore this email.\n"
                ),
            ),
            TransactionalTemplate::VerificationRevoked { domain } => (
                "Your domain verification was removed".to_string(),
                format!(
                    "Verification for {domain} on the Anseo benchmark has been removed.\n\n\
                     If you believe this is an error, you can re-start the claim flow at any time.\n"
                ),
            ),
        };
        content_policy::assert_no_promotion(&subject, &body)?;
        Ok(Message {
            stream: Stream::Transactional,
            from: from.to_string(),
            to: to.to_string(),
            subject,
            body,
        })
    }
}

/// Inputs for a marketing email. All compliance artefacts are required.
#[derive(Debug, Clone)]
pub struct MarketingTemplate {
    /// Honest, non-deceptive subject line (CAN-SPAM §5(a)(2)).
    pub subject: String,
    /// The substantive content (e.g. the rank-change summary).
    pub content: String,
    /// One-click unsubscribe URL — no login required (AC-2/AC-3).
    pub unsubscribe_url: String,
    /// Preference-center URL.
    pub preferences_url: String,
    /// Physical postal address (CAN-SPAM §5(a)(5)).
    pub postal_address: String,
}

impl MarketingTemplate {
    /// A ready-made rank-change marketing template (AC-2).
    pub fn rank_change(
        domain: &str,
        old_rank: u32,
        new_rank: u32,
        unsubscribe_url: String,
        preferences_url: String,
        postal_address: String,
    ) -> Self {
        let direction = if new_rank < old_rank { "up" } else { "down" };
        MarketingTemplate {
            subject: format!("{domain} moved {direction} to #{new_rank} on the Anseo benchmark"),
            content: format!(
                "{domain} moved from #{old_rank} to #{new_rank} on the Anseo benchmark leaderboard."
            ),
            unsubscribe_url,
            preferences_url,
            postal_address,
        }
    }

    /// Build the marketing message, enforcing the compliance-artefact policy.
    ///
    /// `from` must be the marketing subdomain (see [`Stream::subdomain`]).
    pub fn build(&self, from: &str, to: &str) -> Result<Message, TemplateError> {
        if self.unsubscribe_url.trim().is_empty() {
            return Err(TemplateError::MissingUnsubscribeLink);
        }
        if self.postal_address.trim().is_empty() {
            return Err(TemplateError::MissingPostalAddress);
        }
        if self.preferences_url.trim().is_empty() {
            return Err(TemplateError::MissingPreferenceLink);
        }
        let body = format!(
            "{content}\n\n\
             ----\n\
             Unsubscribe (one click, no login): {unsub}\n\
             Manage your preferences: {prefs}\n\
             {postal}\n",
            content = self.content,
            unsub = self.unsubscribe_url,
            prefs = self.preferences_url,
            postal = self.postal_address,
        );
        Ok(Message {
            stream: Stream::Marketing,
            from: from.to_string(),
            to: to.to_string(),
            subject: self.subject.clone(),
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transactional_verification_has_only_its_single_purpose() {
        let msg = TransactionalTemplate::DomainVerification {
            verify_url: "https://benchmark.anseo.dev/verify/abc".into(),
        }
        .build("verify@mail.benchmark.anseo.dev", "owner@acme.com")
        .unwrap();
        assert_eq!(msg.stream, Stream::Transactional);
        assert!(msg.body.contains("https://benchmark.anseo.dev/verify/abc"));
        // No promotional footer, no unsubscribe (transactional cannot be
        // unsubscribed — it carries the requested thing).
        assert!(!msg.body.to_lowercase().contains("unsubscribe"));
    }

    #[test]
    fn content_policy_rejects_promo_in_transactional() {
        // Simulate a promo string sneaking into a transactional body.
        let err = content_policy::assert_no_promotion(
            "Verify your domain",
            "Verify now and get 20% off your first month — limited time!",
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TemplateError::PromotionalContentInTransactional { .. }
        ));
    }

    #[test]
    fn content_policy_allows_clean_transactional() {
        assert!(content_policy::assert_no_promotion(
            "Verify your domain",
            "Open this link to complete verification: https://x/verify/abc",
        )
        .is_ok());
    }

    #[test]
    fn marketing_requires_unsubscribe_postal_and_preferences() {
        let base = MarketingTemplate {
            subject: "acme.com moved up to #3".into(),
            content: "acme.com moved from #5 to #3.".into(),
            unsubscribe_url: "https://x/u/tok".into(),
            preferences_url: "https://x/preferences/tok".into(),
            postal_address: "Anseo Inc, 1 Main St, Dublin, Ireland".into(),
        };
        // Happy path.
        let msg = base.build("news@mail.x", "owner@acme.com").unwrap();
        assert!(msg.body.contains("https://x/u/tok"));
        assert!(msg.body.contains("Dublin"));
        assert!(msg.body.contains("https://x/preferences/tok"));

        // Missing each artefact fails closed.
        let mut no_unsub = base.clone();
        no_unsub.unsubscribe_url = "  ".into();
        assert_eq!(
            no_unsub.build("f", "t").unwrap_err(),
            TemplateError::MissingUnsubscribeLink
        );

        let mut no_postal = base.clone();
        no_postal.postal_address = "".into();
        assert_eq!(
            no_postal.build("f", "t").unwrap_err(),
            TemplateError::MissingPostalAddress
        );

        let mut no_prefs = base.clone();
        no_prefs.preferences_url = "".into();
        assert_eq!(
            no_prefs.build("f", "t").unwrap_err(),
            TemplateError::MissingPreferenceLink
        );
    }

    #[test]
    fn rank_change_subject_is_honest_about_direction() {
        let up = MarketingTemplate::rank_change(
            "acme.com",
            5,
            3,
            "https://x/u/t".into(),
            "https://x/preferences/t".into(),
            "Anseo Inc, Dublin".into(),
        );
        assert!(up.subject.contains("up"));
        let down = MarketingTemplate::rank_change(
            "acme.com",
            3,
            7,
            "https://x/u/t".into(),
            "https://x/preferences/t".into(),
            "Anseo Inc, Dublin".into(),
        );
        assert!(down.subject.contains("down"));
    }
}
