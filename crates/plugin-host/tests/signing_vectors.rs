//! Story 17.3 `[plg-2]` — the §5.4 signature test-vector corpus:
//! valid, tampered manifest, tampered entrypoint, revoked key, revoked plugin,
//! rotated key WITH a valid rotation claim, rotated key WITHOUT a claim, and an
//! untrusted (non-root-signed) namespace claim.

use anseo_plugin_host::signing::{
    signing_digest, verify_signed_plugin, NamespaceClaim, RevocationList, SignatureStatus,
    SignedPlugin, SigningError,
};
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;

fn gen_key() -> SigningKey {
    let mut seed = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut seed);
    SigningKey::from_bytes(&seed)
}

const MANIFEST: &[u8] = b"id = \"priya.perplexity-pro\"\nversion = \"0.3.1\"\n";
const ENTRYPOINT: &[u8] = b"\0asm\x01\0\0\0fake-wasm-bytes";

struct Author {
    key: SigningKey,
}
impl Author {
    fn new() -> Self {
        Author { key: gen_key() }
    }
    fn pubkey(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }
    fn sign_plugin(&self, manifest: &[u8], entry: &[u8]) -> [u8; 64] {
        self.key.sign(&signing_digest(manifest, entry)).to_bytes()
    }
}

/// Build a claim and have `root` sign it. Returns (claim, claim_signature).
fn make_claim(
    root: &SigningKey,
    author: &Author,
    namespace: &str,
    rotation_of: Option<[u8; 32]>,
) -> (NamespaceClaim, [u8; 64]) {
    let claim = NamespaceClaim {
        namespace: namespace.to_string(),
        keyid: "k1".to_string(),
        author_pubkey: author.pubkey(),
        rotation_of,
    };
    let sig = root.sign(&claim.canonical_bytes()).to_bytes();
    (claim, sig)
}

#[test]
fn valid_signature_verifies_and_pins_on_first_install() {
    let root = gen_key();
    let author = Author::new();
    let (claim, claim_sig) = make_claim(&root, &author, "priya", None);
    let sig = author.sign_plugin(MANIFEST, ENTRYPOINT);

    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.3.1",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };

    let (status, pin) = verify_signed_plugin(
        &plugin,
        &[root.verifying_key().to_bytes()],
        &RevocationList::default(),
        None,
    )
    .unwrap();
    assert_eq!(status, SignatureStatus::Signed);
    assert_eq!(pin, author.pubkey());
    assert_eq!(SignatureStatus::Signed.as_str(), "signed");
}

#[test]
fn tampered_manifest_fails() {
    let root = gen_key();
    let author = Author::new();
    let (claim, claim_sig) = make_claim(&root, &author, "priya", None);
    let sig = author.sign_plugin(MANIFEST, ENTRYPOINT);

    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.3.1",
        manifest_bytes: b"id = \"priya.perplexity-pro\"\nversion = \"9.9.9\"\n",
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let err = verify_signed_plugin(
        &plugin,
        &[root.verifying_key().to_bytes()],
        &RevocationList::default(),
        None,
    )
    .unwrap_err();
    assert_eq!(err, SigningError::BadSignature);
}

#[test]
fn tampered_entrypoint_fails() {
    let root = gen_key();
    let author = Author::new();
    let (claim, claim_sig) = make_claim(&root, &author, "priya", None);
    let sig = author.sign_plugin(MANIFEST, ENTRYPOINT);

    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.3.1",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: b"\0asm\x01\0\0\0EVIL-wasm-bytes",
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let err = verify_signed_plugin(
        &plugin,
        &[root.verifying_key().to_bytes()],
        &RevocationList::default(),
        None,
    )
    .unwrap_err();
    assert_eq!(err, SigningError::BadSignature);
}

#[test]
fn revoked_key_refuses_install() {
    let root = gen_key();
    let author = Author::new();
    let (claim, claim_sig) = make_claim(&root, &author, "priya", None);
    let sig = author.sign_plugin(MANIFEST, ENTRYPOINT);
    let revs = RevocationList {
        revoked_keys: vec![("priya".to_string(), "k1".to_string())],
        ..Default::default()
    };
    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.3.1",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let err =
        verify_signed_plugin(&plugin, &[root.verifying_key().to_bytes()], &revs, None).unwrap_err();
    assert_eq!(
        err,
        SigningError::RevokedKey {
            namespace: "priya".into(),
            keyid: "k1".into()
        }
    );
}

#[test]
fn revoked_plugin_refuses_install() {
    let root = gen_key();
    let author = Author::new();
    let (claim, claim_sig) = make_claim(&root, &author, "priya", None);
    let sig = author.sign_plugin(MANIFEST, ENTRYPOINT);
    let revs = RevocationList {
        revoked_plugins: vec![("priya.perplexity-pro".to_string(), "0.3.1".to_string())],
        ..Default::default()
    };
    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.3.1",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let err =
        verify_signed_plugin(&plugin, &[root.verifying_key().to_bytes()], &revs, None).unwrap_err();
    assert_eq!(
        err,
        SigningError::RevokedPlugin {
            plugin_id: "priya.perplexity-pro".into(),
            version: "0.3.1".into()
        }
    );
}

#[test]
fn rotated_key_with_valid_rotation_claim_accepted() {
    let root = gen_key();
    let old_author = Author::new();
    let new_author = Author::new();

    // New key carries a root-signed rotation_of the previously pinned key.
    let (claim, claim_sig) = make_claim(&root, &new_author, "priya", Some(old_author.pubkey()));
    let sig = new_author.sign_plugin(MANIFEST, ENTRYPOINT);
    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.4.0",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let (status, pin) = verify_signed_plugin(
        &plugin,
        &[root.verifying_key().to_bytes()],
        &RevocationList::default(),
        Some(old_author.pubkey()),
    )
    .unwrap();
    assert_eq!(status, SignatureStatus::Signed);
    assert_eq!(
        pin,
        new_author.pubkey(),
        "trust store re-pins to the rotated key"
    );
}

#[test]
fn rotated_key_without_rotation_claim_refused() {
    let root = gen_key();
    let old_author = Author::new();
    let new_author = Author::new();

    // New key, NO rotation_of, even though root-signed.
    let (claim, claim_sig) = make_claim(&root, &new_author, "priya", None);
    let sig = new_author.sign_plugin(MANIFEST, ENTRYPOINT);
    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.4.0",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let err = verify_signed_plugin(
        &plugin,
        &[root.verifying_key().to_bytes()],
        &RevocationList::default(),
        Some(old_author.pubkey()),
    )
    .unwrap_err();
    assert_eq!(err, SigningError::RotationWithoutClaim("priya".into()));
}

#[test]
fn rotation_claiming_wrong_prior_key_is_tofu_mismatch() {
    let root = gen_key();
    let pinned = Author::new();
    let unrelated = Author::new();
    let new_author = Author::new();

    // Rotation claim points at some other key, not the one we have pinned.
    let (claim, claim_sig) = make_claim(&root, &new_author, "priya", Some(unrelated.pubkey()));
    let sig = new_author.sign_plugin(MANIFEST, ENTRYPOINT);
    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.4.0",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let err = verify_signed_plugin(
        &plugin,
        &[root.verifying_key().to_bytes()],
        &RevocationList::default(),
        Some(pinned.pubkey()),
    )
    .unwrap_err();
    assert_eq!(err, SigningError::TofuMismatch("priya".into()));
}

#[test]
fn namespace_claim_not_signed_by_root_is_untrusted() {
    let real_root = gen_key();
    let impostor_root = gen_key();
    let author = Author::new();

    // Claim is signed by the impostor, but we only pin the real root.
    let (claim, claim_sig) = make_claim(&impostor_root, &author, "priya", None);
    let sig = author.sign_plugin(MANIFEST, ENTRYPOINT);
    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.3.1",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let err = verify_signed_plugin(
        &plugin,
        &[real_root.verifying_key().to_bytes()],
        &RevocationList::default(),
        None,
    )
    .unwrap_err();
    assert_eq!(err, SigningError::UntrustedNamespaceClaim("priya".into()));
}

#[test]
fn same_pinned_key_reinstall_succeeds() {
    let root = gen_key();
    let author = Author::new();
    let (claim, claim_sig) = make_claim(&root, &author, "priya", None);
    let sig = author.sign_plugin(MANIFEST, ENTRYPOINT);
    let plugin = SignedPlugin {
        plugin_id: "priya.perplexity-pro",
        version: "0.3.1",
        manifest_bytes: MANIFEST,
        entrypoint_bytes: ENTRYPOINT,
        signature: &sig,
        claim: &claim,
        claim_signature: &claim_sig,
    };
    let (status, _) = verify_signed_plugin(
        &plugin,
        &[root.verifying_key().to_bytes()],
        &RevocationList::default(),
        Some(author.pubkey()),
    )
    .unwrap();
    assert_eq!(status, SignatureStatus::Signed);
}
