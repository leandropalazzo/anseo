//! Envelope encryption for benchmark contributions (Story 39.1, ADR-003).
//!
//! # Why envelope encryption
//!
//! Phase 2 used `ProjectHmac = HMAC(global master_secret, project_id)` to
//! pseudonymize contributions. That only breaks *linkage*: the redacted rows
//! stay fully readable, and a single global secret makes every contribution
//! un-shreddable. To erase one project's contributions you would have to
//! identify and delete every row by hand.
//!
//! Story 39.1 replaces that write path with **envelope encryption**:
//!
//! - Each contribution gets a fresh random 256-bit **DEK** (data encryption
//!   key) drawn from the OS CSPRNG. The DEK encrypts the redacted
//!   [`crate::BenchmarkPayload`] with XChaCha20-Poly1305 (a vetted AEAD).
//! - The DEK is then **wrapped** (encrypted) under a per-project **KEK** (key
//!   encryption key) — also XChaCha20-Poly1305 — and only the wrapped DEK
//!   travels with the ciphertext. The plaintext DEK is never persisted.
//! - The KEK lives in the operator's local [`SecretStore`] chain
//!   (keyring → age-file → in-memory) under the namespace
//!   `benchmark-kek:<project_id>`, kept **distinct** from provider API-key
//!   entries and **outside** any project data directory.
//!
//! Because every contribution for a project is decryptable only through that
//! project's KEK, destroying the single KEK entry cryptographically erases
//! **all** of that project's contributions at once (the actual destroy command
//! is Story 39.2 — this module only structures the KEK so destruction is a
//! one-line `SecretStore::remove`).
//!
//! The [`ProjectHmac`](crate::ProjectHmac) is retained, but **only for
//! linkage** (grouping a project's rows together server-side); it is no longer
//! the erasure mechanism.
//!
//! # The hard gate
//!
//! A contribution cannot be produced unless a per-project KEK exists. This is
//! enforced two ways:
//!
//! - **Type level**: [`ProjectKek`] has no public constructor. The only ways
//!   to obtain one are [`ProjectKek::load`] (fails if absent) and
//!   [`ProjectKek::load_or_create`] (provisions one on first use). A
//!   [`SealedContribution`] can only be built by [`ProjectKek::seal`], so the
//!   compiler guarantees no contribution is sealed without a KEK in hand.
//! - **Runtime**: [`ProjectKek::load`] returns [`CryptoError::KekMissing`]
//!   when the SecretStore has no entry for the project.

use anseo_core::{Secret, SecretStore, SecretStoreError};
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::BenchmarkPayload;

/// Size of a KEK / DEK in bytes (XChaCha20-Poly1305 uses a 256-bit key).
const KEY_LEN: usize = 32;

/// SecretStore namespace prefix for per-project KEKs. Deliberately distinct
/// from provider names (`openai`, `anthropic`, …) so a KEK can never collide
/// with — or be mistaken for — an API key.
///
/// Story 39.1b alignment: this prefix equals [`anseo_core::BENCHMARK_KEK_KEY_PREFIX`]
/// (declared in `anseo-core` to avoid a circular dependency). Both key classes
/// — benchmark KEKs and per-project provider secrets — use the same
/// [`SecretStore`] abstraction and the same concrete backends; they differ only
/// in this namespace (see `anseo_core::secret_store` module docs for the
/// full two-key-class table).
const KEK_NAMESPACE: &str = "benchmark-kek";

/// Build the SecretStore key under which a project's KEK is stored.
pub fn kek_secret_key(project_id: &str) -> String {
    format!("{KEK_NAMESPACE}:{project_id}")
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    /// HARD GATE: no per-project KEK exists, so no contribution may be sealed.
    #[error(
        "no benchmark KEK for project `{project_id}`; a per-project key must be \
         provisioned before contributions can be sealed (run the benchmark opt-in flow)"
    )]
    KekMissing { project_id: String },

    #[error("benchmark KEK for project `{project_id}` is malformed: {reason}")]
    KekMalformed { project_id: String, reason: String },

    /// A fresh KEK was generated but could not be persisted to any DURABLE
    /// backend (keyring or age-file) — only an ephemeral in-memory leg was
    /// available. Returning Ok here would orphan the KEK on the next restart
    /// and render every contribution sealed under it permanently
    /// undecryptable, so creation fails loudly instead.
    #[error(
        "refusing to provision an ephemeral benchmark KEK for project `{project_id}`: no durable \
         secret backend (OS keyring or age-encrypted file) is available, so the key would be lost \
         on restart and its contributions left permanently undecryptable. Configure a keyring or \
         set `{passphrase_env}` for the age-file backend, then retry."
    )]
    EphemeralKek {
        project_id: String,
        passphrase_env: &'static str,
    },

    #[error("secret store error while accessing benchmark KEK: {0}")]
    SecretStore(#[from] SecretStoreError),

    #[error("AEAD encryption failed")]
    Encrypt,

    #[error("AEAD decryption failed (wrong KEK or corrupted ciphertext)")]
    Decrypt,

    #[error("failed to serialize payload for sealing: {0}")]
    Serialize(String),
}

/// A per-project Key Encryption Key.
///
/// There is **no public constructor**. The only ways to obtain a `ProjectKek`
/// are [`ProjectKek::load`] and [`ProjectKek::load_or_create`], both of which
/// go through the operator's [`SecretStore`]. Holding a `ProjectKek` value is
/// therefore proof that a KEK exists for the project — which is exactly the
/// gate Story 39.1 requires before any contribution is produced.
///
/// The key bytes are never serialized, never logged, and never leave this
/// type except to wrap/unwrap DEKs.
pub struct ProjectKek {
    project_id: String,
    key: Key,
}

impl std::fmt::Debug for ProjectKek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectKek")
            .field("project_id", &self.project_id)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl Drop for ProjectKek {
    /// Best-effort scrub of the KEK bytes when the value is dropped, so raw
    /// key material does not linger in freed memory.
    fn drop(&mut self) {
        self.key.as_mut_slice().zeroize();
    }
}

impl ProjectKek {
    /// Load the KEK for `project_id` from the secret store.
    ///
    /// HARD GATE: returns [`CryptoError::KekMissing`] if no KEK has been
    /// provisioned for the project. This is the runtime half of the gate; the
    /// type-level half is that `ProjectKek` cannot be constructed any other
    /// way.
    pub fn load(store: &dyn SecretStore, project_id: &str) -> Result<Self, CryptoError> {
        let secret = match store.get(&kek_secret_key(project_id)) {
            Ok(s) => s,
            Err(SecretStoreError::NotFound { .. }) => {
                return Err(CryptoError::KekMissing {
                    project_id: project_id.to_string(),
                })
            }
            Err(e) => return Err(CryptoError::SecretStore(e)),
        };
        Self::from_secret(project_id, &secret)
    }

    /// Load the KEK for `project_id`, provisioning a fresh random one if none
    /// exists yet. Used by the opt-in flow to establish the key the first time
    /// a project contributes. After this returns, [`ProjectKek::load`] will
    /// succeed for the same project.
    pub fn load_or_create(store: &dyn SecretStore, project_id: &str) -> Result<Self, CryptoError> {
        match Self::load(store, project_id) {
            Ok(kek) => Ok(kek),
            Err(CryptoError::KekMissing { .. }) => {
                let kek = Self::generate(project_id);
                // DURABLE-OR-FAIL: a freshly minted KEK MUST land in a durable
                // backend. `set_durable` skips the ephemeral in-memory leg and
                // returns `NoDurableBackend` when only that leg is available,
                // which we translate to `EphemeralKek` so the caller never
                // silently provisions a key that vanishes on restart (which
                // would render the project's contributions undecryptable).
                match store.set_durable(&kek_secret_key(project_id), kek.to_secret()) {
                    Ok(()) => Ok(kek),
                    Err(SecretStoreError::NoDurableBackend) => Err(CryptoError::EphemeralKek {
                        project_id: project_id.to_string(),
                        passphrase_env: anseo_core::AGE_PASSPHRASE_ENV,
                    }),
                    Err(e) => Err(CryptoError::SecretStore(e)),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// CRYPTO-SHRED (Story 39.2): irreversibly destroy a project's KEK.
    ///
    /// Removes the per-project KEK entry from every leg of the operator's
    /// [`SecretStore`] chain. Because every contribution for the project was
    /// sealed with a DEK wrapped under *only* this KEK, destroying it renders
    /// every wrapped DEK — and therefore every contribution's payload —
    /// **permanently undecryptable**. This is the erasure mechanism for GDPR
    /// Art.17 ("right to erasure"): one `remove` cryptographically erases all
    /// of the project's benchmark contributions at once.
    ///
    /// This is **irreversible**: there is no escrow and no recovery copy of the
    /// KEK. After this returns `Ok`, [`ProjectKek::load`] reports
    /// [`CryptoError::KekMissing`] and no previously sealed contribution can
    /// ever be opened again.
    ///
    /// **Idempotent**: removing a KEK that is already absent is a no-op success
    /// (the SecretStore `remove` contract). This deliberately does not
    /// distinguish "already shredded" from "never existed" — both leave the
    /// project with no recoverable KEK, which is the intended end state.
    ///
    /// HONEST SCOPE: this guarantee covers only key material under OpenGEO's
    /// control (the SecretStore legs). It does **not** reach operator backups,
    /// filesystem/volume snapshots, or database WAL/replication streams that
    /// may hold an earlier copy of the KEK or its sealed contributions — those
    /// are outside OpenGEO's control and out of scope for the cryptographic
    /// guarantee. The opt-out CLI surfaces this explicitly to the operator.
    pub fn destroy(store: &dyn SecretStore, project_id: &str) -> Result<(), CryptoError> {
        store
            .remove(&kek_secret_key(project_id))
            .map_err(CryptoError::SecretStore)
    }

    /// Generate a fresh random KEK in memory (not yet persisted). Private:
    /// callers reach this only through [`ProjectKek::load_or_create`].
    fn generate(project_id: &str) -> Self {
        let key = XChaCha20Poly1305::generate_key(&mut OsRng);
        Self {
            project_id: project_id.to_string(),
            key,
        }
    }

    fn from_secret(project_id: &str, secret: &Secret) -> Result<Self, CryptoError> {
        // Hex-decoded KEK bytes are raw key material: hold them in `Zeroizing`
        // so the heap buffer is scrubbed when this function returns.
        let bytes = Zeroizing::new(hex::decode(secret.expose()).map_err(|e| {
            CryptoError::KekMalformed {
                project_id: project_id.to_string(),
                reason: format!("not valid hex: {e}"),
            }
        })?);
        if bytes.len() != KEY_LEN {
            return Err(CryptoError::KekMalformed {
                project_id: project_id.to_string(),
                reason: format!("expected {KEY_LEN} bytes, got {}", bytes.len()),
            });
        }
        Ok(Self {
            project_id: project_id.to_string(),
            key: *Key::from_slice(&bytes),
        })
    }

    /// Hex-encode the raw key for storage in the SecretStore. The intermediate
    /// hex string is held in `Zeroizing` so it is scrubbed once the `Secret`
    /// has taken its own copy.
    fn to_secret(&self) -> Secret {
        let hex = Zeroizing::new(hex::encode(self.key));
        Secret::new(hex.as_str())
    }

    /// The project this KEK belongs to.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Raw key bytes, for the crate-internal HMAC linkage derivation only.
    /// Deliberately NOT `pub` — never crosses the crate boundary.
    pub(crate) fn linkage_key(&self) -> &[u8] {
        self.key.as_slice()
    }

    /// Envelope-encrypt a redacted payload into an **anonymous-tier**
    /// [`SealedContribution`] (no brand identity attached).
    ///
    /// Generates a fresh random DEK, AEAD-encrypts the serialized payload with
    /// it, then wraps the DEK under this KEK. The plaintext DEK is dropped at
    /// the end of this function and never persisted.
    pub fn seal(&self, payload: &BenchmarkPayload) -> Result<SealedContribution, CryptoError> {
        self.seal_inner(payload, None)
    }

    /// Envelope-encrypt a redacted payload into a **brand-visibility
    /// (identified) tier** [`SealedContribution`] (Story 44.1).
    ///
    /// Identical to [`ProjectKek::seal`] except the resulting contribution
    /// carries a `verification_token` (43.2) that resolves to brand identity
    /// **server-side** via the entity registry. The token is the ONLY identity
    /// carried — the brand name is never present in [`BenchmarkPayload`] nor in
    /// the sealed wire form. APPEARING ≠ CLAIMING.
    ///
    /// HARD GATE (Story 39.1a): identity is transmitted only when this method is
    /// reached, and reaching it requires a live `&self` ([`ProjectKek`]). A
    /// caller without a KEK cannot construct one (no public constructor), and
    /// [`ProjectKek::load`] returns [`CryptoError::KekMissing`] when none is
    /// provisioned — so an identified contribution attempted with no KEK is
    /// refused, never silently skipped.
    pub fn seal_identified(
        &self,
        payload: &BenchmarkPayload,
        verification_token: &str,
    ) -> Result<SealedContribution, CryptoError> {
        self.seal_inner(payload, Some(verification_token))
    }

    fn seal_inner(
        &self,
        payload: &BenchmarkPayload,
        verification_token: Option<&str>,
    ) -> Result<SealedContribution, CryptoError> {
        // Serialized plaintext is confidential redacted data — scrub it on drop.
        let plaintext = Zeroizing::new(
            serde_json::to_vec(payload).map_err(|e| CryptoError::Serialize(e.to_string()))?,
        );

        // Bind both AEAD layers to the project via its linkage HMAC AND — for
        // the identified tier — the verification token, as associated data.
        // Binding the project_hmac ties a wrapped DEK and its payload to *this*
        // project. Binding the verification_token additionally ties the
        // ciphertext to *this* identity: because the token travels in cleartext
        // on `SealedContribution`, an attacker could otherwise swap it for
        // another verified project's token while the ciphertext still decrypts,
        // silently re-attributing the contribution. Including it in the AAD
        // makes any such swap fail the AEAD tag in `open()` (Story 44.1
        // autoreview SECURITY fix). Anonymous-tier seals carry no token, so the
        // AAD is the project_hmac alone — identical to the pre-44.1 binding.
        let aad = aad_bytes(payload.project_hmac().as_hex(), verification_token);
        let aad: &[u8] = &aad;

        // Fresh per-contribution DEK. The `Key` GenericArray itself does not
        // implement `Zeroize`, so copy its bytes into a `Zeroizing` buffer and
        // scrub the original array; both the buffer and the wrapping happen
        // before the DEK leaves scope.
        let mut dek = XChaCha20Poly1305::generate_key(&mut OsRng);
        let dek_bytes = Zeroizing::new(dek.to_vec());
        dek.as_mut_slice().zeroize();

        let dek_cipher = XChaCha20Poly1305::new(Key::from_slice(&dek_bytes));
        let payload_nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let payload_ct = dek_cipher
            .encrypt(
                &payload_nonce,
                Payload {
                    msg: plaintext.as_ref(),
                    aad,
                },
            )
            .map_err(|_| CryptoError::Encrypt)?;

        // Wrap the DEK under the KEK, with the same project-binding AAD.
        let kek_cipher = XChaCha20Poly1305::new(&self.key);
        let dek_nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let wrapped_dek = kek_cipher
            .encrypt(
                &dek_nonce,
                Payload {
                    msg: dek_bytes.as_slice(),
                    aad,
                },
            )
            .map_err(|_| CryptoError::Encrypt)?;

        Ok(SealedContribution {
            project_hmac: payload.project_hmac().as_hex().to_string(),
            wrapped_dek: hex::encode(wrapped_dek),
            dek_nonce: hex::encode(dek_nonce),
            payload_ct: hex::encode(payload_ct),
            payload_nonce: hex::encode(payload_nonce),
            verification_token: verification_token.map(str::to_string),
        })
    }

    /// Reverse of [`ProjectKek::seal`]: unwrap the DEK with this KEK, then
    /// decrypt the payload. Used by tests and by future verification tooling;
    /// the destroy command (39.2) simply removes the KEK so this can no longer
    /// succeed for any of the project's contributions.
    pub fn open(&self, sealed: &SealedContribution) -> Result<BenchmarkPayload, CryptoError> {
        // Reconstruct the exact AAD the seal used: project_hmac plus, for the
        // identified tier, the verification token carried in cleartext on the
        // sealed contribution. If the token has been tampered with (swapped to
        // re-attribute the contribution), the reconstructed AAD no longer
        // matches the one under which the DEK was wrapped, so the AEAD tag check
        // below fails and `open()` rejects the contribution (Story 44.1
        // autoreview SECURITY fix).
        let aad = aad_bytes(&sealed.project_hmac, sealed.verification_token.as_deref());
        let aad: &[u8] = &aad;

        let kek_cipher = XChaCha20Poly1305::new(&self.key);
        let dek_nonce = decode_nonce(&sealed.dek_nonce)?;
        let wrapped_dek = hex::decode(&sealed.wrapped_dek).map_err(|_| CryptoError::Decrypt)?;
        // Unwrapped DEK is raw key material — scrub on drop.
        let dek_bytes = Zeroizing::new(
            kek_cipher
                .decrypt(
                    &dek_nonce,
                    Payload {
                        msg: wrapped_dek.as_ref(),
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Decrypt)?,
        );
        if dek_bytes.len() != KEY_LEN {
            return Err(CryptoError::Decrypt);
        }
        let dek = Key::from_slice(&dek_bytes);

        let dek_cipher = XChaCha20Poly1305::new(dek);
        let payload_nonce = decode_nonce(&sealed.payload_nonce)?;
        let payload_ct = hex::decode(&sealed.payload_ct).map_err(|_| CryptoError::Decrypt)?;
        let plaintext = Zeroizing::new(
            dek_cipher
                .decrypt(
                    &payload_nonce,
                    Payload {
                        msg: payload_ct.as_ref(),
                        aad,
                    },
                )
                .map_err(|_| CryptoError::Decrypt)?,
        );

        serde_json::from_slice(&plaintext).map_err(|_| CryptoError::Decrypt)
    }
}

/// Build the AEAD associated data for a contribution.
///
/// The AAD is the cleartext linkage HMAC, plus — for the identified tier — a
/// domain-separated segment carrying the verification token. The separator
/// byte (`0x1f`, ASCII unit-separator) cannot appear in a hex `project_hmac`,
/// so there is no ambiguity between "project X, no token" and a crafted
/// project_hmac that embeds a token. Anonymous-tier contributions (no token)
/// produce exactly `project_hmac.as_bytes()`, preserving the pre-44.1 binding
/// so existing anonymous contributions still open unchanged.
fn aad_bytes(project_hmac: &str, verification_token: Option<&str>) -> Vec<u8> {
    let mut aad = project_hmac.as_bytes().to_vec();
    if let Some(token) = verification_token {
        aad.push(0x1f);
        aad.extend_from_slice(b"vtok:");
        aad.extend_from_slice(token.as_bytes());
    }
    aad
}

fn decode_nonce(hex_str: &str) -> Result<XNonce, CryptoError> {
    let bytes = hex::decode(hex_str).map_err(|_| CryptoError::Decrypt)?;
    if bytes.len() != 24 {
        return Err(CryptoError::Decrypt);
    }
    Ok(*XNonce::from_slice(&bytes))
}

/// The envelope-encrypted form of one contribution — the only shape that may
/// be persisted or transmitted under the 39.1 write path.
///
/// `project_hmac` is in cleartext **by design**: it is the linkage key that
/// lets the server group a project's contributions without being able to read
/// them. Everything that could be read (the redacted payload) lives only
/// inside `payload_ct`, decryptable solely via the wrapped DEK and therefore
/// solely via the project's KEK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedContribution {
    /// Cleartext linkage identifier (HMAC over the project id). NOT an
    /// erasure mechanism — grouping only.
    pub project_hmac: String,
    /// DEK encrypted under the project KEK (hex).
    pub wrapped_dek: String,
    /// Nonce used to wrap the DEK (hex, 24 bytes).
    pub dek_nonce: String,
    /// Redacted payload encrypted under the DEK (hex).
    pub payload_ct: String,
    /// Nonce used to encrypt the payload (hex, 24 bytes).
    pub payload_nonce: String,
    /// Brand-visibility (identified) tier ONLY (Story 44.1): the verification
    /// token (43.2) that resolves to brand identity server-side. `None` for
    /// anonymous-tier contributions. This is the ONLY identity carried — a raw
    /// brand name is never present here nor in [`BenchmarkPayload`]. APPEARING ≠
    /// CLAIMING.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_token: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RawPromptRun, Redactor, TERMS_VERSION};
    use anseo_core::InMemoryStore;
    use chrono::{TimeZone, Utc};

    const PROJECT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    /// A store that reports itself durable, so `load_or_create`'s durable-or-
    /// fail guard is satisfied without a real keyring/age-file. The ephemeral
    /// path is covered explicitly by `load_or_create_refuses_ephemeral_kek`.
    fn durable_store() -> InMemoryStore {
        InMemoryStore::durable_for_tests()
    }

    fn raw_fixture() -> RawPromptRun {
        RawPromptRun {
            project_id: PROJECT.into(),
            prompt_slug: "vector-db".into(),
            provider: "openai".into(),
            model: "gpt-4o-2024-08-06".into(),
            observed_at: Utc.with_ymd_and_hms(2026, 6, 15, 8, 43, 21).unwrap(),
            observed_rank: Some(2),
            citation_domains: vec!["docs.example.com".into()],
            brand_name: "Pinecone".into(),
            raw_response_text: "Pinecone is a leading vector database…".into(),
            api_key_used: "sk-secret".into(),
            ip_address: "10.0.0.1".into(),
        }
    }

    fn payload(kek: &ProjectKek) -> BenchmarkPayload {
        Redactor::new(kek, TERMS_VERSION)
            .redact(raw_fixture())
            .unwrap()
    }

    #[test]
    fn load_fails_when_no_kek_provisioned() {
        let store = durable_store();
        let err = ProjectKek::load(&store, PROJECT).unwrap_err();
        assert!(matches!(err, CryptoError::KekMissing { .. }));
    }

    #[test]
    fn load_or_create_provisions_then_load_succeeds() {
        let store = durable_store();
        // First call provisions.
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        assert_eq!(kek.project_id(), PROJECT);
        // Now a plain load must succeed and yield the SAME key bytes (so a
        // contribution sealed with one is openable by the other).
        let reloaded = ProjectKek::load(&store, PROJECT).unwrap();
        let sealed = kek.seal(&payload(&kek)).unwrap();
        let opened = reloaded.open(&sealed).unwrap();
        assert_eq!(opened, payload(&kek));
    }

    #[test]
    fn kek_stored_under_distinct_namespace() {
        let store = durable_store();
        ProjectKek::load_or_create(&store, PROJECT).unwrap();
        // Stored under the benchmark-kek namespace, NOT under the bare id or a
        // provider name.
        assert!(store.get(&kek_secret_key(PROJECT)).is_ok());
        assert!(matches!(
            store.get(PROJECT),
            Err(SecretStoreError::NotFound { .. })
        ));
        assert!(matches!(
            store.get("openai"),
            Err(SecretStoreError::NotFound { .. })
        ));
        assert!(kek_secret_key(PROJECT).starts_with("benchmark-kek:"));
    }

    #[test]
    fn seal_open_round_trips() {
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let p = payload(&kek);
        let sealed = kek.seal(&p).unwrap();
        assert_eq!(kek.open(&sealed).unwrap(), p);
    }

    #[test]
    fn sealed_ciphertext_hides_redacted_fields() {
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let sealed = kek.seal(&payload(&kek)).unwrap();
        let wire = serde_json::to_string(&sealed).unwrap();
        // Even the redacted (but still readable) fields must not appear in the
        // sealed wire form — only ciphertext + the linkage HMAC.
        assert!(!wire.contains("vector-db"), "slug leaked: {wire}");
        assert!(!wire.contains("openai"), "provider leaked: {wire}");
        assert!(!wire.contains("gpt-4o"), "model leaked: {wire}");
    }

    #[test]
    fn different_kek_cannot_open() {
        // Destroying a KEK (39.2) makes contributions unrecoverable. Proxy for
        // that here: a different project's KEK cannot open the ciphertext.
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let sealed = kek.seal(&payload(&kek)).unwrap();

        let other = ProjectKek::load_or_create(&store, "01OTHERPROJECTOTHERPROJECT").unwrap();
        assert!(matches!(other.open(&sealed), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn each_seal_uses_fresh_dek_and_nonces() {
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let p = payload(&kek);
        let a = kek.seal(&p).unwrap();
        let b = kek.seal(&p).unwrap();
        // Random DEK + random nonces ⇒ ciphertext and wrapped DEK differ every
        // time, but both still decrypt to the same payload.
        assert_ne!(a.payload_ct, b.payload_ct);
        assert_ne!(a.wrapped_dek, b.wrapped_dek);
        assert_ne!(a.payload_nonce, b.payload_nonce);
        assert_eq!(a.project_hmac, b.project_hmac);
        assert_eq!(kek.open(&a).unwrap(), kek.open(&b).unwrap());
    }

    #[test]
    fn malformed_kek_in_store_is_reported() {
        let store = durable_store();
        store
            .set(&kek_secret_key(PROJECT), Secret::new("not-hex-zzz"))
            .unwrap();
        assert!(matches!(
            ProjectKek::load(&store, PROJECT),
            Err(CryptoError::KekMalformed { .. })
        ));
    }

    #[test]
    fn load_or_create_refuses_ephemeral_kek() {
        // Headless host with NO durable backend: the only store leg is a
        // plain (ephemeral) in-memory store. Provisioning a KEK here would
        // orphan it on restart, so `load_or_create` must REFUSE with
        // `EphemeralKek` rather than silently succeed.
        let ephemeral = InMemoryStore::new();
        let err = ProjectKek::load_or_create(&ephemeral, PROJECT).unwrap_err();
        assert!(
            matches!(err, CryptoError::EphemeralKek { .. }),
            "expected EphemeralKek, got {err:?}"
        );
        // And nothing was written: a subsequent load reports the KEK missing.
        assert!(matches!(
            ProjectKek::load(&ephemeral, PROJECT),
            Err(CryptoError::KekMissing { .. })
        ));

        // Same project, but a durable leg ahead of the ephemeral one: now it
        // succeeds (proving the guard keys on durability, not on store type).
        let chain = anseo_core::ChainedStore::new(vec![
            Box::new(InMemoryStore::durable_for_tests()),
            Box::new(InMemoryStore::new()),
        ]);
        ProjectKek::load_or_create(&chain, PROJECT).unwrap();
    }

    #[test]
    fn destroy_removes_the_kek_then_seal_gate_trips() {
        // CRYPTO-SHRED: after destroy, the KEK is gone, so the hard gate that
        // demands a KEK before any contribution can be sealed must trip with
        // `KekMissing` (the runtime half of the 39.1 gate).
        let store = durable_store();
        ProjectKek::load_or_create(&store, PROJECT).unwrap();
        assert!(store.get(&kek_secret_key(PROJECT)).is_ok());

        ProjectKek::destroy(&store, PROJECT).unwrap();

        assert!(matches!(
            store.get(&kek_secret_key(PROJECT)),
            Err(SecretStoreError::NotFound { .. })
        ));
        // Cannot obtain a ProjectKek to seal with anymore.
        assert!(matches!(
            ProjectKek::load(&store, PROJECT),
            Err(CryptoError::KekMissing { .. })
        ));
    }

    #[test]
    fn destroy_is_idempotent() {
        // Destroying an already-absent KEK is a no-op success: we deliberately
        // do not distinguish "already shredded" from "never existed".
        let store = durable_store();
        // Never provisioned: still Ok.
        ProjectKek::destroy(&store, PROJECT).unwrap();
        // Provision, destroy, destroy again: all Ok.
        ProjectKek::load_or_create(&store, PROJECT).unwrap();
        ProjectKek::destroy(&store, PROJECT).unwrap();
        ProjectKek::destroy(&store, PROJECT).unwrap();
    }

    #[test]
    fn destroy_makes_previously_sealed_contributions_unrecoverable() {
        // The erasure proof: seal a contribution, destroy the KEK, then show
        // the sealed ciphertext can no longer be opened by ANY means — not
        // even by re-provisioning a fresh KEK for the same project id, because
        // the new KEK is random and unrelated to the wrapped DEK.
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let sealed = kek.seal(&payload(&kek)).unwrap();
        // Sanity: openable while the KEK lives.
        assert!(kek.open(&sealed).is_ok());

        ProjectKek::destroy(&store, PROJECT).unwrap();
        // The original handle is gone after destroy in the real flow; emulate
        // an operator who later re-opts-in (fresh random KEK, same project id).
        let resurrected = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        assert!(
            matches!(resurrected.open(&sealed), Err(CryptoError::Decrypt)),
            "a fresh KEK must NOT be able to open contributions sealed under the destroyed KEK"
        );
    }

    #[test]
    fn destroy_targets_only_the_named_project() {
        // Shredding one project must not erase a sibling project's KEK.
        let store = durable_store();
        const OTHER: &str = "01OTHERPROJECTOTHERPROJECT";
        ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let other = ProjectKek::load_or_create(&store, OTHER).unwrap();
        let other_sealed = other
            .seal(
                &Redactor::new(&other, TERMS_VERSION)
                    .redact(RawPromptRun {
                        project_id: OTHER.into(),
                        ..raw_fixture()
                    })
                    .unwrap(),
            )
            .unwrap();

        ProjectKek::destroy(&store, PROJECT).unwrap();

        // Sibling KEK survives and still opens its own contribution.
        let other_reloaded = ProjectKek::load(&store, OTHER).unwrap();
        assert!(other_reloaded.open(&other_sealed).is_ok());
    }

    #[test]
    fn anonymous_seal_carries_no_verification_token() {
        // Story 44.1 AC2: the anonymous tier never attaches identity.
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let sealed = kek.seal(&payload(&kek)).unwrap();
        assert_eq!(sealed.verification_token, None);
        // And it is omitted from the wire form entirely.
        let wire = serde_json::to_string(&sealed).unwrap();
        assert!(
            !wire.contains("verification_token"),
            "anonymous wire must not mention verification_token: {wire}"
        );
    }

    #[test]
    fn identified_seal_carries_token_only_no_brand_name() {
        // Story 44.1 AC2: identity is carried ONLY via the verification token.
        // The brand name ("Pinecone" in the fixture) must never appear, even
        // though the identified tier is active.
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let token = "vtok-43-2-resolves-server-side";
        let sealed = kek.seal_identified(&payload(&kek), token).unwrap();

        assert_eq!(sealed.verification_token.as_deref(), Some(token));
        let wire = serde_json::to_string(&sealed).unwrap();
        assert!(
            wire.contains(token),
            "token must travel on the wire: {wire}"
        );
        assert!(
            !wire.contains("Pinecone"),
            "brand_name leaked into identified contribution: {wire}"
        );
        // Round-trips through serde with the token preserved.
        let back: SealedContribution = serde_json::from_str(&wire).unwrap();
        assert_eq!(back, sealed);
        // The sealed payload itself still opens to a BenchmarkPayload with no
        // brand_name (the redacted shape is unchanged by the identified tier).
        assert_eq!(kek.open(&sealed).unwrap(), payload(&kek));
    }

    #[test]
    fn identified_contribution_refused_when_kek_missing() {
        // Story 44.1 AC3: with no per-project KEK, an identified contribution is
        // REFUSED with KekMissing — never silently skipped, never partially
        // written. The gate is type-level: you cannot reach seal_identified
        // without a ProjectKek, and load is the only way to get one.
        let store = durable_store();
        let err = ProjectKek::load(&store, PROJECT).unwrap_err();
        assert!(
            matches!(err, CryptoError::KekMissing { .. }),
            "expected KekMissing, got {err:?}"
        );
    }

    #[test]
    fn open_fails_when_identified_token_is_tampered() {
        // Story 44.1 autoreview SECURITY fix: the verification_token rides in
        // cleartext on SealedContribution. It is bound into the AEAD AAD, so
        // swapping it (to silently re-attribute the contribution to a different
        // verified identity) must fail decryption — the wrapped DEK no longer
        // authenticates under the reconstructed AAD.
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let mut sealed = kek
            .seal_identified(&payload(&kek), "vtok-original-identity")
            .unwrap();
        // Sanity: opens while the token is intact.
        assert!(kek.open(&sealed).is_ok());

        // Swap the token to another (e.g. another verified project's) value.
        sealed.verification_token = Some("vtok-attacker-controlled".to_string());
        assert!(
            matches!(kek.open(&sealed), Err(CryptoError::Decrypt)),
            "tampering the verification_token must fail open()"
        );

        // Stripping the token entirely (downgrade to anonymous) also fails:
        // the AAD then omits the token segment the seal bound in.
        sealed.verification_token = None;
        assert!(matches!(kek.open(&sealed), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn anonymous_seal_aad_is_token_free_and_stable() {
        // The anonymous-tier AAD must remain exactly the project_hmac bytes so
        // pre-44.1 anonymous contributions still open. Adding a token to an
        // anonymous seal's wire form must NOT make it open (the seal bound no
        // token, so a later-added token diverges the AAD).
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let mut sealed = kek.seal(&payload(&kek)).unwrap();
        assert_eq!(sealed.verification_token, None);
        assert!(kek.open(&sealed).is_ok());

        sealed.verification_token = Some("vtok-injected".to_string());
        assert!(matches!(kek.open(&sealed), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn open_fails_when_aad_project_hmac_does_not_match() {
        // The AEAD layers are bound to the project via its linkage HMAC as
        // AAD. Grafting a ciphertext under a different project_hmac (without
        // the matching KEK/AAD) must fail decryption — ciphertext cannot be
        // re-homed across contribution contexts.
        let store = durable_store();
        let kek = ProjectKek::load_or_create(&store, PROJECT).unwrap();
        let mut sealed = kek.seal(&payload(&kek)).unwrap();

        // Tamper only the cleartext linkage HMAC (the AAD on decrypt). Even
        // with the correct KEK, the DEK-unwrap AEAD tag now fails.
        sealed.project_hmac = "deadbeef".repeat(8);
        assert!(matches!(kek.open(&sealed), Err(CryptoError::Decrypt)));
    }
}
