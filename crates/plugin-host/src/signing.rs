//! Story 17.3 — Ed25519 + TOFU plugin signing (architecture-phase3-plugin-sdk
//! §5). Verification chain at install time:
//!
//!   1. plugin `(id, version)` is not in the revocation list,
//!   2. signing key `(namespace, keyid)` is not in the revocation list,
//!   3. the namespace claim is signed by a compile-pinned `OPENGEO_ROOT_PUBKEY`
//!      (or a rotation thereof),
//!   4. the plugin's detached signature verifies over
//!      `SHA-256(plugin.toml || entrypoint_bytes)` with the author key,
//!   5. TOFU: on first sight of a namespace the author key is pinned; on
//!      subsequent installs a changed key is refused unless the namespace
//!      claim carries a root-signed `rotation_of` of the pinned key.
//!
//! The crate is offline by construction: the root keys are compile-pinned and
//! the revocation list + trust store are passed in by the caller (the CLI /
//! worker owns the filesystem reads). This keeps the verifier a pure function
//! over bytes, which is what the §5.4 test-vector corpus exercises.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[cfg(feature = "signing-tools")]
use ed25519_dalek::{Signer, SigningKey};

/// `SHA-256(plugin.toml || entrypoint_artifact_bytes)` — the message the
/// author's detached signature covers (§5.4.3).
pub fn signing_digest(manifest_bytes: &[u8], entrypoint_bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(manifest_bytes);
    h.update(entrypoint_bytes);
    h.finalize().into()
}

/// A maintainer-signed statement that `author_pubkey` owns `namespace`. The
/// root signature covers [`NamespaceClaim::canonical_bytes`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceClaim {
    pub namespace: String,
    pub keyid: String,
    /// The author's Ed25519 public key (32 bytes).
    pub author_pubkey: [u8; 32],
    /// When this key supersedes a previous one, the hex of the prior pinned
    /// author public key. A rotation is only honored if this is present *and*
    /// the claim is root-signed (§5.4.4).
    pub rotation_of: Option<[u8; 32]>,
}

impl NamespaceClaim {
    /// Deterministic byte encoding the root signs over. Stable across runs and
    /// machines (no map ordering, no wall clock).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut s = String::new();
        s.push_str("anseo-namespace-claim:v1\n");
        s.push_str(&format!("namespace={}\n", self.namespace));
        s.push_str(&format!("keyid={}\n", self.keyid));
        s.push_str(&format!("pubkey={}\n", hex::encode(self.author_pubkey)));
        s.push_str(&format!(
            "rotation_of={}\n",
            self.rotation_of.map(hex::encode).unwrap_or_default()
        ));
        s.into_bytes()
    }
}

/// Registry-root revocation list (`keys/revoked.toml`, §5.4.6).
#[derive(Debug, Clone, Default)]
pub struct RevocationList {
    /// `(namespace, keyid)` tuples whose key is revoked.
    pub revoked_keys: Vec<(String, String)>,
    /// `(plugin_id, version)` tuples whose artifact is revoked.
    pub revoked_plugins: Vec<(String, String)>,
}

impl RevocationList {
    pub fn key_revoked(&self, namespace: &str, keyid: &str) -> bool {
        self.revoked_keys
            .iter()
            .any(|(n, k)| n == namespace && k == keyid)
    }
    pub fn plugin_revoked(&self, plugin_id: &str, version: &str) -> bool {
        self.revoked_plugins
            .iter()
            .any(|(p, v)| p == plugin_id && v == version)
    }
}

/// Outcome annotation recorded in `installed.toml` (§5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    Signed,
    Unsigned,
    DevLocal,
}

impl SignatureStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignatureStatus::Signed => "signed",
            SignatureStatus::Unsigned => "unsigned",
            SignatureStatus::DevLocal => "dev_local",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SigningError {
    #[error("plugin {plugin_id}@{version} is revoked")]
    RevokedPlugin { plugin_id: String, version: String },
    #[error("signing key {namespace}/{keyid} is revoked")]
    RevokedKey { namespace: String, keyid: String },
    #[error("namespace claim for `{0}` is not signed by any pinned root key")]
    UntrustedNamespaceClaim(String),
    #[error("plugin signature does not verify against the author key")]
    BadSignature,
    #[error("malformed key or signature bytes: {0}")]
    Malformed(String),
    #[error("pinned key for namespace `{0}` changed without a valid rotation claim")]
    TofuMismatch(String),
    #[error("key for namespace `{0}` differs from pin but claim carries no rotation_of")]
    RotationWithoutClaim(String),
}

/// Everything the verifier needs about the plugin being installed.
pub struct SignedPlugin<'a> {
    pub plugin_id: &'a str,
    pub version: &'a str,
    pub manifest_bytes: &'a [u8],
    pub entrypoint_bytes: &'a [u8],
    /// 64-byte detached Ed25519 signature over [`signing_digest`].
    pub signature: &'a [u8],
    pub claim: &'a NamespaceClaim,
    /// 64-byte detached Ed25519 signature over `claim.canonical_bytes()`.
    pub claim_signature: &'a [u8],
}

fn verifying_key(bytes: &[u8; 32]) -> Result<VerifyingKey, SigningError> {
    VerifyingKey::from_bytes(bytes).map_err(|e| SigningError::Malformed(e.to_string()))
}

fn signature(bytes: &[u8]) -> Result<Signature, SigningError> {
    let arr: [u8; 64] = bytes.try_into().map_err(|_| {
        SigningError::Malformed(format!("signature must be 64 bytes, got {}", bytes.len()))
    })?;
    Ok(Signature::from_bytes(&arr))
}

/// Verify a plugin against the §5.4 chain. `root_pubkeys` are the compile-pinned
/// roots ([`pinned_root_pubkeys`]); `pinned_author` is the TOFU pin for this
/// namespace from `trusted_keys.toml`, or `None` on first install.
///
/// On success returns the [`SignatureStatus`] and the author key to pin (the
/// caller writes it to the trust store on first install / after rotation).
pub fn verify_signed_plugin(
    plugin: &SignedPlugin<'_>,
    root_pubkeys: &[[u8; 32]],
    revocations: &RevocationList,
    pinned_author: Option<[u8; 32]>,
) -> Result<(SignatureStatus, [u8; 32]), SigningError> {
    // (1) artifact revocation.
    if revocations.plugin_revoked(plugin.plugin_id, plugin.version) {
        return Err(SigningError::RevokedPlugin {
            plugin_id: plugin.plugin_id.to_string(),
            version: plugin.version.to_string(),
        });
    }
    // (2) key revocation.
    if revocations.key_revoked(&plugin.claim.namespace, &plugin.claim.keyid) {
        return Err(SigningError::RevokedKey {
            namespace: plugin.claim.namespace.clone(),
            keyid: plugin.claim.keyid.clone(),
        });
    }

    // (3) the namespace claim must be root-signed.
    let claim_sig = signature(plugin.claim_signature)?;
    let claim_msg = plugin.claim.canonical_bytes();
    let root_ok = root_pubkeys.iter().any(|rk| {
        verifying_key(rk)
            .map(|vk| vk.verify(&claim_msg, &claim_sig).is_ok())
            .unwrap_or(false)
    });
    if !root_ok {
        return Err(SigningError::UntrustedNamespaceClaim(
            plugin.claim.namespace.clone(),
        ));
    }

    // (4) the plugin signature must verify with the (now root-attested) author key.
    let author_vk = verifying_key(&plugin.claim.author_pubkey)?;
    let digest = signing_digest(plugin.manifest_bytes, plugin.entrypoint_bytes);
    let plugin_sig = signature(plugin.signature)?;
    author_vk
        .verify(&digest, &plugin_sig)
        .map_err(|_| SigningError::BadSignature)?;

    // (5) TOFU: enforce the pinned key, allowing a root-signed rotation.
    if let Some(pinned) = pinned_author {
        if pinned != plugin.claim.author_pubkey {
            match plugin.claim.rotation_of {
                None => {
                    return Err(SigningError::RotationWithoutClaim(
                        plugin.claim.namespace.clone(),
                    ))
                }
                Some(prev) if prev == pinned => { /* root-signed rotation of the pinned key: accept */
                }
                Some(_) => return Err(SigningError::TofuMismatch(plugin.claim.namespace.clone())),
            }
        }
    }

    Ok((SignatureStatus::Signed, plugin.claim.author_pubkey))
}

// ---------------------------------------------------------------------------
// Story 41.4 — signing producers (operationalize signing).
//
// These are the *inverse* of the verification chain above and deliberately live
// in the same module, so a change to a signed-byte layout (e.g.
// `signing_digest` or `NamespaceClaim::canonical_bytes`) updates both the
// producer and the verifier in one place — they can never drift apart. The
// `signing-tools` feature keeps `SigningKey` / `rand` out of the verify-only
// build (the worker / install path only ever *verifies*).
// ---------------------------------------------------------------------------

/// Errors from the signing producers (keygen / sign).
#[cfg(feature = "signing-tools")]
#[derive(Debug, Error)]
pub enum SignError {
    #[error("malformed key material: {0}")]
    Malformed(String),
}

/// A freshly generated or loaded Ed25519 signing keypair, as raw bytes.
///
/// `secret` is the 32-byte Ed25519 seed (NOT the expanded 64-byte form); this
/// is what `ed25519_dalek::SigningKey::from_bytes` consumes and what we persist
/// to the GitHub Actions secret. `public` is the 32-byte verifying key, ready to
/// be pinned as `ANSEO_ROOT_PUBKEY` (hex) or written into a `claim.toml`.
#[cfg(feature = "signing-tools")]
#[derive(Clone)]
pub struct Keypair {
    pub secret: [u8; 32],
    pub public: [u8; 32],
}

#[cfg(feature = "signing-tools")]
impl Keypair {
    /// Generate a new random keypair from a CSPRNG.
    pub fn generate() -> Self {
        let sk = SigningKey::generate(&mut rand::rngs::OsRng);
        Keypair {
            secret: sk.to_bytes(),
            public: sk.verifying_key().to_bytes(),
        }
    }

    /// Reconstruct a keypair from a 32-byte secret seed (hex-decoded by the
    /// caller). The public key is derived, so a stored secret is sufficient.
    pub fn from_secret_bytes(secret: [u8; 32]) -> Self {
        let sk = SigningKey::from_bytes(&secret);
        Keypair {
            secret,
            public: sk.verifying_key().to_bytes(),
        }
    }

    fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.secret)
    }

    /// Hex of the public key — the value pinned as `ANSEO_ROOT_PUBKEY`.
    pub fn public_hex(&self) -> String {
        hex::encode(self.public)
    }

    /// Hex of the secret seed — the value stored as the
    /// `ANSEO_PLUGIN_SIGNING_KEY` GitHub Actions secret. NEVER commit this.
    pub fn secret_hex(&self) -> String {
        hex::encode(self.secret)
    }
}

/// Sign the author's detached plugin signature over
/// `SHA-256(manifest_bytes || entrypoint_bytes)` — the exact message
/// [`verify_signed_plugin`] step (4) checks. Returns the raw 64-byte signature
/// that the registry stores as `signature.bin`.
#[cfg(feature = "signing-tools")]
pub fn sign_plugin(author: &Keypair, manifest_bytes: &[u8], entrypoint_bytes: &[u8]) -> [u8; 64] {
    let digest = signing_digest(manifest_bytes, entrypoint_bytes);
    author.signing_key().sign(&digest).to_bytes()
}

/// Sign a namespace claim with the root key, over [`NamespaceClaim::canonical_bytes`]
/// — the exact message [`verify_signed_plugin`] step (3) checks against the
/// pinned roots. Returns the raw 64-byte signature stored (hex-encoded) as the
/// `signature` field of `claim.toml`.
#[cfg(feature = "signing-tools")]
pub fn sign_namespace_claim(root: &Keypair, claim: &NamespaceClaim) -> [u8; 64] {
    root.signing_key().sign(&claim.canonical_bytes()).to_bytes()
}

#[cfg(all(test, feature = "signing-tools"))]
mod sign_roundtrip_tests {
    use super::*;

    /// A plugin signed by `sign_plugin` + a claim root-signed by
    /// `sign_namespace_claim` must pass the full `verify_signed_plugin` chain.
    /// This is the load-bearing guarantee for 41.4: bundles the CI produces
    /// verify under the same code the worker / install path runs.
    #[test]
    fn sign_then_verify_roundtrips() {
        let root = Keypair::generate();
        let author = Keypair::generate();

        let manifest = b"name: anseo/demo\nversion: 1.0.0\n";
        let entrypoint = b"\0asm\x01\0\0\0fake-wasm-bytes";

        let claim = NamespaceClaim {
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_pubkey: author.public,
            rotation_of: None,
        };
        let claim_sig = sign_namespace_claim(&root, &claim);
        let plugin_sig = sign_plugin(&author, manifest, entrypoint);

        let signed = SignedPlugin {
            plugin_id: "anseo/demo",
            version: "1.0.0",
            manifest_bytes: manifest,
            entrypoint_bytes: entrypoint,
            signature: &plugin_sig,
            claim: &claim,
            claim_signature: &claim_sig,
        };

        // First install (no pin yet) verifies and returns the key to pin.
        let (status, pin) =
            verify_signed_plugin(&signed, &[root.public], &RevocationList::default(), None)
                .expect("freshly signed bundle must verify");
        assert_eq!(status, SignatureStatus::Signed);
        assert_eq!(pin, author.public);

        // A second install with the pinned key still verifies.
        verify_signed_plugin(
            &signed,
            &[root.public],
            &RevocationList::default(),
            Some(author.public),
        )
        .expect("pinned re-install of the same key must verify");
    }

    /// A bit-flip in the plugin signature is rejected (AC4: integrity check).
    #[test]
    fn tampered_signature_is_rejected() {
        let root = Keypair::generate();
        let author = Keypair::generate();
        let manifest = b"name: anseo/demo\nversion: 1.0.0\n";
        let entrypoint = b"wasm";
        let claim = NamespaceClaim {
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_pubkey: author.public,
            rotation_of: None,
        };
        let claim_sig = sign_namespace_claim(&root, &claim);
        let mut plugin_sig = sign_plugin(&author, manifest, entrypoint);
        plugin_sig[0] ^= 0x01; // flip a bit

        let signed = SignedPlugin {
            plugin_id: "anseo/demo",
            version: "1.0.0",
            manifest_bytes: manifest,
            entrypoint_bytes: entrypoint,
            signature: &plugin_sig,
            claim: &claim,
            claim_signature: &claim_sig,
        };
        let err = verify_signed_plugin(&signed, &[root.public], &RevocationList::default(), None)
            .expect_err("bit-flipped signature must not verify");
        assert_eq!(err, SigningError::BadSignature);
    }

    /// A manifest changed after signing breaks the digest, so the signature no
    /// longer verifies (AC4: tampered manifest).
    #[test]
    fn tampered_manifest_is_rejected() {
        let root = Keypair::generate();
        let author = Keypair::generate();
        let manifest = b"name: anseo/demo\nversion: 1.0.0\n";
        let entrypoint = b"wasm";
        let claim = NamespaceClaim {
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_pubkey: author.public,
            rotation_of: None,
        };
        let claim_sig = sign_namespace_claim(&root, &claim);
        let plugin_sig = sign_plugin(&author, manifest, entrypoint);

        let tampered = b"name: anseo/demo\nversion: 9.9.9\n"; // edited after signing
        let signed = SignedPlugin {
            plugin_id: "anseo/demo",
            version: "1.0.0",
            manifest_bytes: tampered,
            entrypoint_bytes: entrypoint,
            signature: &plugin_sig,
            claim: &claim,
            claim_signature: &claim_sig,
        };
        let err = verify_signed_plugin(&signed, &[root.public], &RevocationList::default(), None)
            .expect_err("tampered manifest must not verify");
        assert_eq!(err, SigningError::BadSignature);
    }

    /// A claim signed by a non-root key is untrusted (the root attestation is
    /// what gates first-party trust). TODO(key-rotation): rotation test vectors
    /// when the rotation story lands.
    #[test]
    fn claim_not_signed_by_root_is_untrusted() {
        let real_root = Keypair::generate();
        let impostor = Keypair::generate();
        let author = Keypair::generate();
        let manifest = b"name: anseo/demo\nversion: 1.0.0\n";
        let entrypoint = b"wasm";
        let claim = NamespaceClaim {
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_pubkey: author.public,
            rotation_of: None,
        };
        // Signed by the impostor, not the pinned real root.
        let claim_sig = sign_namespace_claim(&impostor, &claim);
        let plugin_sig = sign_plugin(&author, manifest, entrypoint);
        let signed = SignedPlugin {
            plugin_id: "anseo/demo",
            version: "1.0.0",
            manifest_bytes: manifest,
            entrypoint_bytes: entrypoint,
            signature: &plugin_sig,
            claim: &claim,
            claim_signature: &claim_sig,
        };
        let err = verify_signed_plugin(
            &signed,
            &[real_root.public],
            &RevocationList::default(),
            None,
        )
        .expect_err("claim not root-signed must be untrusted");
        assert!(matches!(err, SigningError::UntrustedNamespaceClaim(_)));
    }

    /// `from_secret_bytes` round-trips: a key reconstructed from its stored seed
    /// produces signatures that verify (this is what CI does — loads the secret
    /// from the GitHub Actions secret and signs).
    #[test]
    fn keypair_from_stored_secret_signs_verifiably() {
        let original = Keypair::generate();
        let reloaded = Keypair::from_secret_bytes(original.secret);
        assert_eq!(original.public, reloaded.public);

        let author = Keypair::generate();
        let claim = NamespaceClaim {
            namespace: "anseo".into(),
            keyid: "root-2026".into(),
            author_pubkey: author.public,
            rotation_of: None,
        };
        let claim_sig = sign_namespace_claim(&reloaded, &claim);
        let manifest = b"m";
        let entrypoint = b"e";
        let plugin_sig = sign_plugin(&author, manifest, entrypoint);
        let signed = SignedPlugin {
            plugin_id: "anseo/demo",
            version: "1.0.0",
            manifest_bytes: manifest,
            entrypoint_bytes: entrypoint,
            signature: &plugin_sig,
            claim: &claim,
            claim_signature: &claim_sig,
        };
        verify_signed_plugin(
            &signed,
            &[original.public],
            &RevocationList::default(),
            None,
        )
        .expect("reloaded-secret signature must verify against the original public key");
    }
}

/// The compile-pinned first-party root public keys (§5.4.2). Set
/// `ANSEO_ROOT_PUBKEY` at build time to a comma-separated list of 64-char hex
/// Ed25519 public keys (multiple supported for rotation). Empty when unset.
/// The deprecated name `OPENGEO_ROOT_PUBKEY` is also checked for back-compat.
pub fn pinned_root_pubkeys() -> Vec<[u8; 32]> {
    match option_env!("ANSEO_ROOT_PUBKEY").or(option_env!("OPENGEO_ROOT_PUBKEY")) {
        None => Vec::new(),
        Some(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|h| {
                let bytes = hex::decode(h).ok()?;
                <[u8; 32]>::try_from(bytes.as_slice()).ok()
            })
            .collect(),
    }
}
