//! Recipient hashing — GDPR data-minimisation.
//!
//! Suppression and the audit log never store cleartext email. They key on
//! `sha256(normalized email)`. Normalization is lowercase + trim so that
//! `Foo@Example.com ` and `foo@example.com` collapse to one identity.

use sha2::{Digest, Sha256};

/// Normalize an email for hashing: trim surrounding whitespace, lowercase.
///
/// We deliberately do NOT do gmail-style dot/plus folding — that would merge
/// distinct mailboxes the user may treat as separate, and over-suppression is
/// a deliverability/compliance risk (you'd silently drop mail someone asked
/// for). One address → one identity.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// `sha256(normalize_email(raw))`, lowercase hex. This is the stable recipient
/// identity used by the suppression list and the audit log.
pub fn recipient_hash(raw_email: &str) -> String {
    let normalized = normalize_email(raw_email);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_case_and_whitespace_insensitive() {
        let a = recipient_hash("Foo@Example.com");
        let b = recipient_hash("  foo@example.com ");
        assert_eq!(a, b);
        // 64 hex chars = 32 bytes sha256.
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn distinct_addresses_have_distinct_hashes() {
        assert_ne!(recipient_hash("a@x.com"), recipient_hash("b@x.com"));
        // No plus-folding: tag addresses are distinct identities.
        assert_ne!(recipient_hash("a+tag@x.com"), recipient_hash("a@x.com"));
    }

    #[test]
    fn hash_never_contains_cleartext() {
        let h = recipient_hash("secret@example.com");
        assert!(!h.contains("secret"));
        assert!(!h.contains("example"));
    }
}
