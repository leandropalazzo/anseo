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
        s.push_str("opengeo-namespace-claim:v1\n");
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

/// The compile-pinned first-party root public keys (§5.4.2). Set
/// `OPENGEO_ROOT_PUBKEY` at build time to a comma-separated list of 64-char hex
/// Ed25519 public keys (multiple supported for rotation). Empty when unset.
pub fn pinned_root_pubkeys() -> Vec<[u8; 32]> {
    match option_env!("OPENGEO_ROOT_PUBKEY") {
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
