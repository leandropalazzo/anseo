//! Story 17.5 — filesystem view of a plugin registry.
//!
//! A registry is a directory tree (the GitHub-backed registry is the same tree
//! served over `raw.githubusercontent.com`; `ogeo plugin` fetches it into a
//! local cache and then reads it through this same [`FsRegistry`], so the
//! resolver logic is transport-agnostic and offline-testable):
//!
//! ```text
//! <root>/index.toml                                  # search index
//! <root>/plugins/<id>/<version>/manifest.yaml        # PluginManifest
//! <root>/plugins/<id>/<version>/entrypoint.wasm      # artifact bytes
//! <root>/plugins/<id>/<version>/signature.bin        # 64-byte Ed25519 sig
//! <root>/plugins/<id>/<version>/claim.toml           # namespace claim + sig
//! <root>/keys/revoked.toml                           # revocation list
//! ```

use std::path::{Path, PathBuf};

use opengeo_plugin_host::signing::RevocationList;
use opengeo_plugin_manifest::PluginManifest;
use serde::Deserialize;

use super::plugin_install::PluginError;

/// One row of `index.toml` (`[[plugin]]` array entry).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct Index {
    #[serde(default)]
    plugin: Vec<IndexEntry>,
}

/// `claim.toml` — the per-version namespace claim, hex-encoded.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaimFile {
    pub namespace: String,
    pub keyid: String,
    /// 64-char hex Ed25519 author public key.
    pub author_pubkey: String,
    /// 64-char hex of the prior pinned key, when this is a rotation.
    #[serde(default)]
    pub rotation_of: Option<String>,
    /// 128-char hex root signature over the claim's canonical bytes.
    pub signature: String,
}

#[derive(Debug, Deserialize, Default)]
struct RevokedFile {
    #[serde(default)]
    revoked_keys: Vec<(String, String)>,
    #[serde(default)]
    revoked_plugins: Vec<(String, String)>,
}

/// Everything install needs for one `(id, version)`.
pub struct PluginArtifacts {
    pub manifest_bytes: Vec<u8>,
    pub manifest: PluginManifest,
    pub entrypoint_bytes: Vec<u8>,
    /// Absent for an unsigned plugin (install requires `--allow-unsigned`).
    pub signature: Option<Vec<u8>>,
    pub claim: Option<ClaimFile>,
}

pub struct FsRegistry {
    root: PathBuf,
}

impl FsRegistry {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FsRegistry { root: root.into() }
    }

    fn version_dir(&self, id: &str, version: &str) -> PathBuf {
        self.root.join("plugins").join(id).join(version)
    }

    fn read_index(&self) -> Result<Index, PluginError> {
        let path = self.root.join("index.toml");
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| PluginError::Registry(format!("read {}: {e}", path.display())))?;
        toml::from_str(&raw).map_err(|e| PluginError::Registry(format!("parse index.toml: {e}")))
    }

    /// `ogeo plugin search <query>` — substring match over id + description.
    pub fn search(&self, query: &str) -> Result<Vec<IndexEntry>, PluginError> {
        let q = query.to_lowercase();
        Ok(self
            .read_index()?
            .plugin
            .into_iter()
            .filter(|e| {
                e.id.to_lowercase().contains(&q) || e.description.to_lowercase().contains(&q)
            })
            .collect())
    }

    /// Resolve the index entry for `(id, version)`; `version = "latest"` picks
    /// the highest-listed version for the id.
    pub fn resolve(&self, id: &str, version: &str) -> Result<IndexEntry, PluginError> {
        let mut matches: Vec<IndexEntry> = self
            .read_index()?
            .plugin
            .into_iter()
            .filter(|e| e.id == id)
            .collect();
        if matches.is_empty() {
            return Err(PluginError::NotFound {
                id: id.to_string(),
                version: version.to_string(),
            });
        }
        if version == "latest" {
            matches.sort_by(|a, b| a.version.cmp(&b.version));
            return Ok(matches.pop().unwrap());
        }
        matches
            .into_iter()
            .find(|e| e.version == version)
            .ok_or_else(|| PluginError::NotFound {
                id: id.to_string(),
                version: version.to_string(),
            })
    }

    pub fn fetch(&self, id: &str, version: &str) -> Result<PluginArtifacts, PluginError> {
        let dir = self.version_dir(id, version);
        let manifest_bytes = read(&dir.join("manifest.yaml"))?;
        let manifest = PluginManifest::load_from_path(&dir.join("manifest.yaml"))
            .map_err(|e| PluginError::Registry(format!("manifest: {e}")))?;
        let entrypoint_bytes = read(&dir.join("entrypoint.wasm"))?;
        let signature = read_optional(&dir.join("signature.bin"))?;
        let claim = match read_optional_string(&dir.join("claim.toml"))? {
            Some(raw) => Some(
                toml::from_str(&raw)
                    .map_err(|e| PluginError::Registry(format!("parse claim.toml: {e}")))?,
            ),
            None => None,
        };
        Ok(PluginArtifacts {
            manifest_bytes,
            manifest,
            entrypoint_bytes,
            signature,
            claim,
        })
    }

    /// The registry-root revocation list (`keys/revoked.toml`). Absent ⇒ empty.
    pub fn revocations(&self) -> Result<RevocationList, PluginError> {
        let path = self.root.join("keys").join("revoked.toml");
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RevocationList::default())
            }
            Err(e) => return Err(PluginError::Registry(format!("read revoked.toml: {e}"))),
        };
        let parsed: RevokedFile = toml::from_str(&raw)
            .map_err(|e| PluginError::Registry(format!("parse revoked.toml: {e}")))?;
        Ok(RevocationList {
            revoked_keys: parsed.revoked_keys,
            revoked_plugins: parsed.revoked_plugins,
        })
    }
}

fn read(path: &Path) -> Result<Vec<u8>, PluginError> {
    std::fs::read(path).map_err(|e| PluginError::Registry(format!("read {}: {e}", path.display())))
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, PluginError> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PluginError::Registry(format!(
            "read {}: {e}",
            path.display()
        ))),
    }
}

fn read_optional_string(path: &Path) -> Result<Option<String>, PluginError> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PluginError::Registry(format!(
            "read {}: {e}",
            path.display()
        ))),
    }
}
