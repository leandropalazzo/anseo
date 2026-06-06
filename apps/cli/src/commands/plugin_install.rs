//! Story 17.5 — `ogeo plugin {install, list, remove, upgrade}` runtime.
//!
//! The pipeline is split out from the clap handlers so it is unit-testable
//! against a fixture registry + ephemeral DB (`#[sqlx::test]`): the handlers in
//! [`super::plugin`] are thin wrappers that supply the real registry, the
//! `~/.config/opengeo` home, and the compile-pinned root keys.
//!
//! Install (signed path, arch §5.4):
//!   1. fetch artifacts from the registry,
//!   2. verify against the revocation list + root-signed namespace claim + the
//!      author signature + the TOFU pin (`anseo_plugin_host::signing`),
//!   3. materialize the bundle under `<home>/plugins/<id>/<version>/`,
//!   4. pin the namespace key in `trusted_keys.toml` and record the install in
//!      `installed.toml`,
//!   5. write the `plugin_installs` audit row.
//!
//! `--allow-unsigned` skips step 2, records `signature_status = "unsigned"`
//! (FR-55), and is the only way to install a plugin with no signature.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anseo_plugin_host::capability::{upgrade_plan, CapabilitySet};
use anseo_plugin_host::signing::{
    verify_signed_plugin, NamespaceClaim, SignatureStatus, SignedPlugin,
};
use anseo_storage::repositories::plugin_installs::{NewPluginInstall, PluginInstallsRepo};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use super::plugin_registry::{ClaimFile, FsRegistry};

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("registry error: {0}")]
    Registry(String),
    #[error("plugin {id}@{version} not found in registry index")]
    NotFound { id: String, version: String },
    #[error("unsigned plugin {0} refuses to install without --allow-unsigned")]
    UnsignedRefused(String),
    #[error("first-party plugin must be signed: {0}")]
    FirstPartyMustBeSigned(String),
    #[error("signature verification failed: {0}")]
    Verify(String),
    #[error("malformed registry key/signature: {0}")]
    Malformed(String),
    #[error("upgrade adds capabilities {0:?}; re-run with --accept-new-capabilities")]
    NewCapabilities(Vec<String>),
    #[error("no active install found for `{0}`")]
    NotInstalled(String),
    #[error("filesystem error: {0}")]
    Io(String),
    #[error("database error: {0}")]
    Db(String),
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub allow_unsigned: bool,
    pub accept_new_capabilities: bool,
    pub actor: String,
}

impl Default for InstallOptions {
    fn default() -> Self {
        InstallOptions {
            allow_unsigned: false,
            accept_new_capabilities: false,
            actor: "local".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub id: String,
    pub version: String,
    pub signature_status: SignatureStatus,
    pub install_dir: PathBuf,
    pub audit_id: Uuid,
}

/// One row of `<home>/installed.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledEntry {
    pub id: String,
    pub version: String,
    pub signature_status: String,
    #[serde(default)]
    pub namespace: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct InstalledFile {
    #[serde(default)]
    plugin: Vec<InstalledEntry>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TrustedKeysFile {
    /// namespace → hex Ed25519 public key.
    #[serde(default)]
    keys: BTreeMap<String, String>,
}

fn hex32(s: &str) -> Result<[u8; 32], PluginError> {
    let b = hex::decode(s).map_err(|e| PluginError::Malformed(e.to_string()))?;
    <[u8; 32]>::try_from(b.as_slice())
        .map_err(|_| PluginError::Malformed("expected 32-byte key".into()))
}

fn claim_to_namespace_claim(c: &ClaimFile) -> Result<NamespaceClaim, PluginError> {
    Ok(NamespaceClaim {
        namespace: c.namespace.clone(),
        keyid: c.keyid.clone(),
        author_pubkey: hex32(&c.author_pubkey)?,
        rotation_of: c.rotation_of.as_deref().map(hex32).transpose()?,
    })
}

// ---------------- install ----------------

pub async fn install_plugin(
    pool: &PgPool,
    registry: &FsRegistry,
    home: &Path,
    id: &str,
    version: &str,
    opts: &InstallOptions,
    root_pubkeys: &[[u8; 32]],
) -> Result<InstallOutcome, PluginError> {
    let entry = registry.resolve(id, version)?;
    let version = entry.version;
    let art = registry.fetch(id, &version)?;

    // Story 41.4 — first-party plugins (publisher = "anseo.ai") are
    // signature-REQUIRED. There is no `--allow-unsigned` escape hatch for them:
    // a missing signature/claim is a hard error, and an invalid signature is
    // caught by `verify_signed_plugin` below.
    let is_first_party = art.manifest.is_first_party();
    if is_first_party && (art.signature.is_none() || art.claim.is_none()) {
        return Err(PluginError::FirstPartyMustBeSigned(id.to_string()));
    }

    // For community plugins with no signature, `--allow-unsigned` records an
    // unsigned install with a warning (the worker still gates load separately).
    let community_unsigned = opts.allow_unsigned && !is_first_party;

    let (status, publisher_fp, trust_root, namespace) = if community_unsigned {
        eprintln!(
            "[UNSIGNED PLUGIN] {id}: installing without a verified signature \
             (--allow-unsigned). Anseo cannot attest this plugin's authenticity."
        );
        (
            "unsigned",
            "unsigned".to_string(),
            "unsigned".to_string(),
            String::new(),
        )
    } else {
        let claim = art
            .claim
            .as_ref()
            .ok_or_else(|| PluginError::UnsignedRefused(id.to_string()))?;
        let signature = art
            .signature
            .as_ref()
            .ok_or_else(|| PluginError::UnsignedRefused(id.to_string()))?;
        let ns_claim = claim_to_namespace_claim(claim)?;
        let claim_sig = hex::decode(&claim.signature)
            .map_err(|e| PluginError::Malformed(format!("claim signature: {e}")))?;

        let signed = SignedPlugin {
            plugin_id: id,
            version: &version,
            manifest_bytes: &art.manifest_bytes,
            entrypoint_bytes: &art.entrypoint_bytes,
            signature,
            claim: &ns_claim,
            claim_signature: &claim_sig,
        };
        let pinned = read_trusted_keys(home)?.keys.get(&claim.namespace).cloned();
        let pinned = match pinned {
            Some(h) => Some(hex32(&h)?),
            None => None,
        };
        let revocations = registry.revocations()?;
        let (_status, pin) = verify_signed_plugin(&signed, root_pubkeys, &revocations, pinned)
            .map_err(|e| PluginError::Verify(e.to_string()))?;
        pin_trusted_key(home, &claim.namespace, &hex::encode(pin))?;
        (
            "signed",
            hex::encode(pin),
            "first-party-root".to_string(),
            claim.namespace.clone(),
        )
    };

    // Materialize the bundle.
    let install_dir = home.join("plugins").join(id).join(&version);
    std::fs::create_dir_all(&install_dir).map_err(|e| PluginError::Io(e.to_string()))?;
    std::fs::write(install_dir.join("manifest.yaml"), &art.manifest_bytes)
        .map_err(|e| PluginError::Io(e.to_string()))?;
    std::fs::write(install_dir.join("entrypoint.wasm"), &art.entrypoint_bytes)
        .map_err(|e| PluginError::Io(e.to_string()))?;

    record_installed(
        home,
        InstalledEntry {
            id: id.to_string(),
            version: version.clone(),
            signature_status: status.to_string(),
            namespace,
        },
    )?;

    // Audit row.
    let capability_set = serde_json::to_value(&art.manifest.capabilities)
        .map_err(|e| PluginError::Db(e.to_string()))?;
    let repo = PluginInstallsRepo::new(pool);
    let audit_id = repo
        .insert(NewPluginInstall {
            plugin_name: id,
            plugin_version: &version,
            publisher_pubkey_fingerprint: &publisher_fp,
            installed_by_actor: &opts.actor,
            capability_set,
            signature_verified: status == "signed",
            signing_trust_root: &trust_root,
        })
        .await
        .map_err(|e| PluginError::Db(e.to_string()))?;

    let signature_status = match status {
        "signed" => SignatureStatus::Signed,
        _ => SignatureStatus::Unsigned,
    };
    Ok(InstallOutcome {
        id: id.to_string(),
        version,
        signature_status,
        install_dir,
        audit_id,
    })
}

// ---------------- list / remove / upgrade ----------------

pub async fn list_installed(
    pool: &PgPool,
) -> Result<Vec<anseo_storage::repositories::plugin_installs::PluginInstallRow>, PluginError> {
    PluginInstallsRepo::new(pool)
        .find_active()
        .await
        .map_err(|e| PluginError::Db(e.to_string()))
}

pub async fn remove_plugin(pool: &PgPool, home: &Path, id: &str) -> Result<(), PluginError> {
    let removed = PluginInstallsRepo::new(pool)
        .mark_removed(id, Some("ogeo plugin remove"))
        .await
        .map_err(|e| PluginError::Db(e.to_string()))?;
    if removed == 0 {
        return Err(PluginError::NotInstalled(id.to_string()));
    }
    // Drop the on-disk bundle + the installed.toml entry.
    let dir = home.join("plugins").join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| PluginError::Io(e.to_string()))?;
    }
    let mut installed = read_installed(home)?;
    installed.plugin.retain(|e| e.id != id);
    write_installed(home, &installed)?;
    Ok(())
}

/// §6.4 — refuses unless the new version's capability set is a subset of the
/// old one, or `--accept-new-capabilities` is passed. On accept it installs the
/// new version (which re-runs full signature verification).
pub async fn upgrade_plugin(
    pool: &PgPool,
    registry: &FsRegistry,
    home: &Path,
    id: &str,
    new_version: &str,
    opts: &InstallOptions,
    root_pubkeys: &[[u8; 32]],
) -> Result<InstallOutcome, PluginError> {
    let installed = read_installed(home)?;
    let current = installed
        .plugin
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| PluginError::NotInstalled(id.to_string()))?;

    let old_manifest = anseo_plugin_manifest::PluginManifest::load_from_path(
        &home
            .join("plugins")
            .join(id)
            .join(&current.version)
            .join("manifest.yaml"),
    )
    .map_err(|e| PluginError::Io(format!("read installed manifest: {e}")))?;
    let resolved = registry.resolve(id, new_version)?;
    let new_art = registry.fetch(id, &resolved.version)?;

    let old_caps = CapabilitySet::new(old_manifest.capabilities);
    let new_caps = CapabilitySet::new(new_art.manifest.capabilities.clone());
    if let Err(refused) = upgrade_plan(&old_caps, &new_caps, opts.accept_new_capabilities) {
        return Err(PluginError::NewCapabilities(refused.added));
    }

    install_plugin(
        pool,
        registry,
        home,
        id,
        &resolved.version,
        opts,
        root_pubkeys,
    )
    .await
}

// ---------------- on-disk state ----------------

fn read_installed(home: &Path) -> Result<InstalledFile, PluginError> {
    match std::fs::read_to_string(home.join("installed.toml")) {
        Ok(raw) => {
            toml::from_str(&raw).map_err(|e| PluginError::Io(format!("installed.toml: {e}")))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(InstalledFile::default()),
        Err(e) => Err(PluginError::Io(e.to_string())),
    }
}

fn write_installed(home: &Path, file: &InstalledFile) -> Result<(), PluginError> {
    std::fs::create_dir_all(home).map_err(|e| PluginError::Io(e.to_string()))?;
    let raw = toml::to_string_pretty(file).map_err(|e| PluginError::Io(e.to_string()))?;
    std::fs::write(home.join("installed.toml"), raw).map_err(|e| PluginError::Io(e.to_string()))
}

fn record_installed(home: &Path, entry: InstalledEntry) -> Result<(), PluginError> {
    let mut file = read_installed(home)?;
    file.plugin.retain(|e| e.id != entry.id);
    file.plugin.push(entry);
    write_installed(home, &file)
}

fn read_trusted_keys(home: &Path) -> Result<TrustedKeysFile, PluginError> {
    match std::fs::read_to_string(home.join("trusted_keys.toml")) {
        Ok(raw) => {
            toml::from_str(&raw).map_err(|e| PluginError::Io(format!("trusted_keys.toml: {e}")))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(TrustedKeysFile::default()),
        Err(e) => Err(PluginError::Io(e.to_string())),
    }
}

fn pin_trusted_key(home: &Path, namespace: &str, pubkey_hex: &str) -> Result<(), PluginError> {
    let mut file = read_trusted_keys(home)?;
    file.keys
        .insert(namespace.to_string(), pubkey_hex.to_string());
    std::fs::create_dir_all(home).map_err(|e| PluginError::Io(e.to_string()))?;
    let raw = toml::to_string_pretty(&file).map_err(|e| PluginError::Io(e.to_string()))?;
    std::fs::write(home.join("trusted_keys.toml"), raw).map_err(|e| PluginError::Io(e.to_string()))
}
