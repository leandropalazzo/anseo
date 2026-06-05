//! Preference-center tokens — no-login access to manage marketing preferences
//! and one-click unsubscribe (AC-3).
//!
//! A token is the bearer of authority for exactly one recipient. We never
//! store the raw token: the DB holds `sha256(raw)` (same posture as the
//! verification magic links in 43.2). The raw token lives only in the email
//! link. Lookup is by hash; comparison is constant-time via [`subtle`].
//!
//! Tokens are derived with HMAC-SHA256 over a per-deployment secret so that an
//! attacker who learns the hashing scheme still cannot forge a valid token
//! without the secret. The raw token is `hex(hmac(secret, recipient_hash ||
//! nonce))`.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// A freshly-minted token: the `raw` value goes in the email link, the
/// `hash` goes in `comms_preference_tokens.token_hash`.
#[derive(Debug, Clone)]
pub struct MintedToken {
    /// Raw token — embed in the link, never persist.
    pub raw: String,
    /// `sha256(raw)` — persist this.
    pub hash: String,
}

/// Mint a preference token for a recipient.
///
/// `secret` is the per-deployment signing secret. `recipient_hash` binds the
/// token to one recipient. `nonce` makes each minted token unique even for the
/// same recipient (rotation, re-send). Callers typically pass a fresh UUID as
/// the nonce.
pub fn mint(secret: &[u8], recipient_hash: &str, nonce: &str) -> MintedToken {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts keys of any length");
    mac.update(recipient_hash.as_bytes());
    mac.update(b"|");
    mac.update(nonce.as_bytes());
    let raw = hex::encode(mac.finalize().into_bytes());
    let hash = hash_token(&raw);
    MintedToken { raw, hash }
}

/// `sha256(raw)`, lowercase hex — the DB lookup key.
pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

/// Constant-time comparison of two token hashes. Used when verifying a
/// presented token against a stored hash to avoid timing leaks.
pub fn hashes_match(a: &str, b: &str) -> bool {
    let abytes = a.as_bytes();
    let bbytes = b.as_bytes();
    if abytes.len() != bbytes.len() {
        return false;
    }
    abytes.ct_eq(bbytes).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_is_deterministic_for_same_inputs() {
        let a = mint(b"secret", "rh", "nonce-1");
        let b = mint(b"secret", "rh", "nonce-1");
        assert_eq!(a.raw, b.raw);
        assert_eq!(a.hash, b.hash);
    }

    #[test]
    fn nonce_changes_the_token() {
        let a = mint(b"secret", "rh", "nonce-1");
        let b = mint(b"secret", "rh", "nonce-2");
        assert_ne!(a.raw, b.raw);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn secret_changes_the_token() {
        let a = mint(b"secret-a", "rh", "n");
        let b = mint(b"secret-b", "rh", "n");
        assert_ne!(a.raw, b.raw);
    }

    #[test]
    fn hash_of_raw_matches_minted_hash() {
        let t = mint(b"secret", "rh", "n");
        assert_eq!(hash_token(&t.raw), t.hash);
    }

    #[test]
    fn hashes_match_is_constant_time_correct() {
        let t = mint(b"secret", "rh", "n");
        assert!(hashes_match(&t.hash, &hash_token(&t.raw)));
        assert!(!hashes_match(&t.hash, "deadbeef"));
        // length mismatch is rejected without panic.
        assert!(!hashes_match(&t.hash, "short"));
    }
}
