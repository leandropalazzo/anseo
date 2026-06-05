//! Story 17.5 `[plg-4]` + `[plg-6]` — marketplace install golden test against a
//! fixture registry, and the `--allow-unsigned` audit-row test.
//!
//! These drive the install pipeline directly (not through the binary) so they
//! can assert both the on-disk file layout and the `plugin_installs` audit row
//! against an ephemeral `#[sqlx::test]` database.

use std::path::Path;

use anseo_cli::commands::plugin_install::{install_plugin, list_installed, InstallOptions};
use anseo_cli::commands::plugin_registry::FsRegistry;
use anseo_plugin_host::signing::{NamespaceClaim, SignatureStatus};
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use sqlx::PgPool;

const PLUGIN_ID: &str = "priya.perplexity-pro-extractor";
const VERSION: &str = "0.3.1";

const MANIFEST_YAML: &str = "\
name: priya.perplexity-pro-extractor
version: 0.3.1
description: Perplexity Pro citation extractor
author: priya
homepage: https://priya.dev
capabilities:
  - kind: network
    allowlist: [\"api.priya.dev\"]
plugin_type: extractor
entry_point: entrypoint.wasm
";
const ENTRYPOINT: &[u8] = b"\0asm\x01\0\0\0fixture-extractor";

fn gen_key() -> SigningKey {
    let mut seed = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut seed);
    SigningKey::from_bytes(&seed)
}

fn digest(manifest: &[u8], entry: &[u8]) -> [u8; 32] {
    anseo_plugin_host::signing::signing_digest(manifest, entry)
}

/// Build a fixture registry tree under `root`. Returns the root public key the
/// installer must be configured with. If `signed` is false, no signature/claim
/// files are written (an unsigned plugin).
fn build_fixture_registry(root: &Path, signed: bool) -> [u8; 32] {
    let root_key = gen_key();
    let author = gen_key();

    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("index.toml"),
        format!(
            "[[plugin]]\nid = \"{PLUGIN_ID}\"\nversion = \"{VERSION}\"\ndescription = \"Perplexity Pro citation extractor\"\n"
        ),
    )
    .unwrap();

    let dir = root.join("plugins").join(PLUGIN_ID).join(VERSION);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.yaml"), MANIFEST_YAML).unwrap();
    std::fs::write(dir.join("entrypoint.wasm"), ENTRYPOINT).unwrap();

    if signed {
        let sig = author
            .sign(&digest(MANIFEST_YAML.as_bytes(), ENTRYPOINT))
            .to_bytes();
        std::fs::write(dir.join("signature.bin"), sig).unwrap();

        let claim = NamespaceClaim {
            namespace: "priya".to_string(),
            keyid: "k1".to_string(),
            author_pubkey: author.verifying_key().to_bytes(),
            rotation_of: None,
        };
        let claim_sig = root_key.sign(&claim.canonical_bytes()).to_bytes();
        std::fs::write(
            dir.join("claim.toml"),
            format!(
                "namespace = \"priya\"\nkeyid = \"k1\"\nauthor_pubkey = \"{}\"\nsignature = \"{}\"\n",
                hex::encode(claim.author_pubkey),
                hex::encode(claim_sig)
            ),
        )
        .unwrap();
    }

    root_key.verifying_key().to_bytes()
}

#[sqlx::test(migrations = "../../crates/storage/migrations")]
async fn plg_4_marketplace_install_golden(pool: PgPool) {
    let tmp = tempfile::tempdir().unwrap();
    let registry_root = tmp.path().join("registry");
    let home = tmp.path().join("home");
    let root_pub = build_fixture_registry(&registry_root, true);

    let registry = FsRegistry::new(&registry_root);
    let outcome = install_plugin(
        &pool,
        &registry,
        &home,
        PLUGIN_ID,
        VERSION,
        &InstallOptions::default(),
        &[root_pub],
    )
    .await
    .expect("signed install should succeed");

    assert_eq!(outcome.signature_status, SignatureStatus::Signed);

    // File layout (§5.4 install).
    let vdir = home.join("plugins").join(PLUGIN_ID).join(VERSION);
    assert!(vdir.join("manifest.yaml").exists(), "manifest materialized");
    assert!(
        vdir.join("entrypoint.wasm").exists(),
        "entrypoint materialized"
    );
    assert!(
        home.join("installed.toml").exists(),
        "installed.toml written"
    );
    let trusted = std::fs::read_to_string(home.join("trusted_keys.toml")).unwrap();
    assert!(trusted.contains("priya"), "namespace key pinned (TOFU)");

    // Audit row.
    let rows = list_installed(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].plugin_name, PLUGIN_ID);
    assert_eq!(rows[0].plugin_version, VERSION);
    assert!(rows[0].signature_verified, "signed install → verified");
    assert_eq!(rows[0].signing_trust_root, "first-party-root");
}

#[sqlx::test(migrations = "../../crates/storage/migrations")]
async fn plg_6_allow_unsigned_audits_unsigned_status(pool: PgPool) {
    let tmp = tempfile::tempdir().unwrap();
    let registry_root = tmp.path().join("registry");
    let home = tmp.path().join("home");
    // Unsigned fixture: no signature.bin / claim.toml.
    let root_pub = build_fixture_registry(&registry_root, false);

    let registry = FsRegistry::new(&registry_root);

    // Without --allow-unsigned, an unsigned plugin refuses to install.
    let refused = install_plugin(
        &pool,
        &registry,
        &home,
        PLUGIN_ID,
        VERSION,
        &InstallOptions::default(),
        &[root_pub],
    )
    .await;
    assert!(
        refused.is_err(),
        "unsigned install must refuse without --allow-unsigned"
    );

    // With --allow-unsigned it installs and records signature_status=unsigned.
    let opts = InstallOptions {
        allow_unsigned: true,
        ..Default::default()
    };
    let outcome = install_plugin(
        &pool,
        &registry,
        &home,
        PLUGIN_ID,
        VERSION,
        &opts,
        &[root_pub],
    )
    .await
    .expect("--allow-unsigned install should succeed");
    assert_eq!(outcome.signature_status, SignatureStatus::Unsigned);

    let rows = list_installed(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].signature_verified, "unsigned → not verified");
    assert_eq!(rows[0].signing_trust_root, "unsigned");
}
