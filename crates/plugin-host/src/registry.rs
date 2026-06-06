//! Story 41.1 — live, configurable plugin registry client.
//!
//! This operationalizes the Phase-3 GitHub flat-file registry
//! (architecture-phase3 `AD-Phase3-PluginRegistry`, plugin-sdk §3.1). The
//! registry is a directory tree served either over GitHub's CDN
//! (`raw.githubusercontent.com/anseo/plugin-registry/...`) or from a local
//! path. The on-disk shape mirrors the resolver in
//! `apps/cli/src/commands/plugin_registry.rs`:
//!
//! ```text
//! <base>/index.toml                                  # registry root index
//! <base>/plugins/<id>/<version>/manifest.yaml        # PluginManifest (YAML)
//! <base>/plugins/<id>/<version>/entrypoint.wasm      # artifact bytes
//! <base>/plugins/<id>/<version>/signature.bin        # 64-byte Ed25519 sig
//! <base>/plugins/<id>/<version>/claim.toml           # namespace claim + sig
//! <base>/keys/revoked.toml                           # revocation list
//! ```
//!
//! ## Transport injection (hermetic tests)
//!
//! Transport is abstracted behind [`RegistryTransport`]. Production uses
//! [`HttpTransport`] (configurable base URL, default the canonical
//! `anseo/plugin-registry` raw URL). Tests use [`FileTransport`] against a
//! temp-dir fixture, so the test suite never touches the network.
//!
//! ## Verification
//!
//! Before an artifact is accepted, [`RegistryClient::fetch_verified`]:
//!
//!   1. recomputes the SHA-256 of the entrypoint bytes and compares it to the
//!      checksum advertised in `index.toml` (integrity / corruption guard), then
//!   2. runs the full §5.4 Ed25519 + TOFU verification chain via
//!      [`crate::signing::verify_signed_plugin`].
//!
//! Tampered-checksum and bad-signature artifacts are rejected.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use anseo_plugin_manifest::PluginManifest;

use crate::signing::{
    verify_signed_plugin, NamespaceClaim, RevocationList, SignatureStatus, SignedPlugin,
    SigningError,
};

/// Canonical registry base, used when nothing is configured. Points at the
/// raw CDN view of the `main` branch of `anseo/plugin-registry` — the single
/// place the live registry URL is declared (Story 41.1 AC5).
pub const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/anseo/plugin-registry/main";

/// Environment variable that overrides the registry base URL.
pub const REGISTRY_URL_ENV: &str = "ANSEO_PLUGIN_REGISTRY_URL";

/// Deprecated alias for [`REGISTRY_URL_ENV`]. Accepted for one release.
#[deprecated(since = "0.7.0", note = "use ANSEO_PLUGIN_REGISTRY_URL instead")]
pub const REGISTRY_URL_ENV_LEGACY: &str = "OPENGEO_PLUGIN_REGISTRY_URL";

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("transport error fetching `{path}`: {source}")]
    Transport {
        path: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("`{path}` not found in registry")]
    NotFound { path: String },
    #[error("failed to parse `{path}`: {message}")]
    Parse { path: String, message: String },
    #[error("plugin `{id}@{version}` not present in index")]
    UnknownPlugin { id: String, version: String },
    #[error(
        "checksum mismatch for `{id}@{version}`: index declares sha256:{expected}, \
         downloaded artifact is sha256:{actual}"
    )]
    ChecksumMismatch {
        id: String,
        version: String,
        expected: String,
        actual: String,
    },
    #[error("plugin `{id}@{version}` is missing a signature or namespace claim")]
    Unsigned { id: String, version: String },
    #[error("signature verification failed for `{id}@{version}`: {source}")]
    Verification {
        id: String,
        version: String,
        #[source]
        source: SigningError,
    },
    #[error("malformed registry data for `{id}@{version}`: {message}")]
    Malformed {
        id: String,
        version: String,
        message: String,
    },
}

/// Source of raw registry bytes, addressed by a path relative to the registry
/// base. Implementations MUST NOT assume a particular transport so the resolver
/// stays transport-agnostic and offline-testable.
pub trait RegistryTransport {
    /// Fetch the bytes at `rel_path` (e.g. `"index.toml"`,
    /// `"plugins/priya.x/0.3.1/entrypoint.wasm"`). Return
    /// [`RegistryError::NotFound`] when the object does not exist.
    fn fetch(&self, rel_path: &str) -> Result<Vec<u8>, RegistryError>;
}

/// HTTP transport over GitHub's raw CDN (or any base URL). Synchronous by
/// design: install is a short, blocking, user-driven action and the host crate
/// otherwise has no async runtime.
pub struct HttpTransport {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl HttpTransport {
    /// Build a transport rooted at `base_url` (no trailing slash required).
    pub fn new(base_url: impl Into<String>) -> Self {
        HttpTransport {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::blocking::Client::new(),
        }
    }

    /// Build a transport from `ANSEO_PLUGIN_REGISTRY_URL` (or the deprecated
    /// `OPENGEO_PLUGIN_REGISTRY_URL`), falling back to [`DEFAULT_REGISTRY_URL`].
    pub fn from_env() -> Self {
        let base = std::env::var(REGISTRY_URL_ENV)
            .or_else(|_| std::env::var("OPENGEO_PLUGIN_REGISTRY_URL"))
            .unwrap_or_else(|_| DEFAULT_REGISTRY_URL.into());
        Self::new(base)
    }
}

impl RegistryTransport for HttpTransport {
    fn fetch(&self, rel_path: &str) -> Result<Vec<u8>, RegistryError> {
        let url = format!("{}/{}", self.base_url, rel_path.trim_start_matches('/'));
        let resp = self
            .client
            .get(&url)
            .send()
            .map_err(|source| RegistryError::Transport {
                path: rel_path.to_string(),
                source: Box::new(source),
            })?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(RegistryError::NotFound {
                path: rel_path.to_string(),
            });
        }
        let resp = resp
            .error_for_status()
            .map_err(|source| RegistryError::Transport {
                path: rel_path.to_string(),
                source: Box::new(source),
            })?;
        resp.bytes()
            .map(|b| b.to_vec())
            .map_err(|source| RegistryError::Transport {
                path: rel_path.to_string(),
                source: Box::new(source),
            })
    }
}

/// Filesystem transport over a local registry tree. Used by hermetic tests and
/// for `file://` / on-disk caches. The registry layout is identical to the
/// HTTP layout, so resolver logic is shared verbatim.
pub struct FileTransport {
    root: PathBuf,
}

impl FileTransport {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        FileTransport { root: root.into() }
    }
}

impl RegistryTransport for FileTransport {
    fn fetch(&self, rel_path: &str) -> Result<Vec<u8>, RegistryError> {
        // Reject traversal: a relative registry path must stay under the root.
        let rel = Path::new(rel_path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(RegistryError::Transport {
                path: rel_path.to_string(),
                source: "path escapes registry root".into(),
            });
        }
        let full = self.root.join(rel);
        match std::fs::read(&full) {
            Ok(b) => Ok(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(RegistryError::NotFound {
                path: rel_path.to_string(),
            }),
            Err(e) => Err(RegistryError::Transport {
                path: rel_path.to_string(),
                source: Box::new(e),
            }),
        }
    }
}

/// In-memory transport — a path→bytes map. Convenient for table-driven tests
/// without touching the filesystem at all.
#[derive(Default)]
pub struct InMemoryTransport {
    objects: Mutex<HashMap<String, Vec<u8>>>,
}

impl InMemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }
    /// Insert/overwrite an object at `rel_path`.
    pub fn insert(&self, rel_path: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.objects
            .lock()
            .expect("poisoned")
            .insert(rel_path.into(), bytes.into());
    }
}

impl RegistryTransport for InMemoryTransport {
    fn fetch(&self, rel_path: &str) -> Result<Vec<u8>, RegistryError> {
        self.objects
            .lock()
            .expect("poisoned")
            .get(rel_path)
            .cloned()
            .ok_or_else(|| RegistryError::NotFound {
                path: rel_path.to_string(),
            })
    }
}

/// One `[[plugin]]` row of `index.toml`. The canonical registry index (§3.1)
/// carries an entry per plugin id with the list of versions and per-version
/// integrity metadata. We keep the fields this client needs and ignore the
/// rest, so the schema can grow without breaking older hosts.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Lowercase hex SHA-256 of the entrypoint artifact. The client refuses an
    /// artifact whose recomputed digest does not match.
    pub sha256: String,
    #[serde(default)]
    pub yanked: bool,
}

#[derive(Debug, Deserialize, Default)]
struct IndexFile {
    #[serde(default)]
    plugin: Vec<IndexEntry>,
}

/// `claim.toml` — the per-version namespace claim, hex-encoded (§5.4).
#[derive(Debug, Clone, Deserialize)]
struct ClaimFile {
    namespace: String,
    keyid: String,
    /// 64-char hex Ed25519 author public key.
    author_pubkey: String,
    /// 64-char hex of the prior pinned key, when this version rotates the key.
    #[serde(default)]
    rotation_of: Option<String>,
    /// 128-char hex root signature over the claim's canonical bytes.
    signature: String,
}

#[derive(Debug, Deserialize, Default)]
struct RevokedFile {
    #[serde(default)]
    revoked_keys: Vec<(String, String)>,
    #[serde(default)]
    revoked_plugins: Vec<(String, String)>,
}

/// A registry artifact that has passed checksum + signature verification.
#[derive(Debug)]
pub struct VerifiedPlugin {
    pub id: String,
    pub version: String,
    pub manifest: PluginManifest,
    pub manifest_bytes: Vec<u8>,
    pub entrypoint_bytes: Vec<u8>,
    /// Verified signature status; always [`SignatureStatus::Signed`] here since
    /// unsigned artifacts are rejected by this method.
    pub status: SignatureStatus,
    /// The author key to pin in the trust store (first install) or re-pin
    /// (after a verified rotation). The caller owns the trust-store write.
    pub author_key_to_pin: [u8; 32],
}

/// The live registry client. Generic over [`RegistryTransport`] so production
/// (HTTP) and tests (file / in-memory) share one code path.
pub struct RegistryClient<T: RegistryTransport> {
    transport: T,
}

impl RegistryClient<HttpTransport> {
    /// Build a client whose base URL comes from the environment
    /// (`ANSEO_PLUGIN_REGISTRY_URL`, or the deprecated
    /// `OPENGEO_PLUGIN_REGISTRY_URL`), defaulting to [`DEFAULT_REGISTRY_URL`].
    pub fn from_env() -> Self {
        RegistryClient {
            transport: HttpTransport::from_env(),
        }
    }

    /// Build a client against an explicit base URL.
    pub fn with_url(base_url: impl Into<String>) -> Self {
        RegistryClient {
            transport: HttpTransport::new(base_url),
        }
    }
}

impl<T: RegistryTransport> RegistryClient<T> {
    /// Build a client over an arbitrary transport (used by tests).
    pub fn new(transport: T) -> Self {
        RegistryClient { transport }
    }

    fn index(&self) -> Result<IndexFile, RegistryError> {
        let raw = self.transport.fetch("index.toml")?;
        let text = String::from_utf8(raw).map_err(|e| RegistryError::Parse {
            path: "index.toml".into(),
            message: format!("not UTF-8: {e}"),
        })?;
        toml::from_str(&text).map_err(|e| RegistryError::Parse {
            path: "index.toml".into(),
            message: e.to_string(),
        })
    }

    /// `anseo plugin search` — substring match over id + description, skipping
    /// yanked rows.
    pub fn search(&self, query: &str) -> Result<Vec<IndexEntry>, RegistryError> {
        let q = query.to_lowercase();
        Ok(self
            .index()?
            .plugin
            .into_iter()
            .filter(|e| !e.yanked)
            .filter(|e| {
                e.id.to_lowercase().contains(&q) || e.description.to_lowercase().contains(&q)
            })
            .collect())
    }

    /// Like [`search`](Self::search), but a **malformed or non-UTF-8
    /// `index.toml`** degrades gracefully to an *empty* index rather than
    /// erroring (Story 41.1 AC6 + Notes: "Malformed TOML must degrade
    /// gracefully — log error + treat as empty index"). A transport/network
    /// failure (e.g. the registry being unreachable) still propagates so the
    /// caller can render the AC4 "registry unreachable" message.
    pub fn search_lenient(&self, query: &str) -> Result<Vec<IndexEntry>, RegistryError> {
        match self.search(query) {
            Ok(hits) => Ok(hits),
            // A parse failure is treated as an empty registry, not a hard error.
            Err(RegistryError::Parse { path, message }) => {
                eprintln!(
                    "warning: registry index `{path}` is malformed ({message}); \
                     treating as empty"
                );
                Ok(Vec::new())
            }
            Err(other) => Err(other),
        }
    }

    /// Resolve the index entry for `(id, version)`. `version = "latest"` picks
    /// the highest non-yanked version listed for the id.
    pub fn resolve(&self, id: &str, version: &str) -> Result<IndexEntry, RegistryError> {
        let mut matches: Vec<IndexEntry> = self
            .index()?
            .plugin
            .into_iter()
            .filter(|e| e.id == id && !e.yanked)
            .collect();
        if matches.is_empty() {
            return Err(RegistryError::UnknownPlugin {
                id: id.to_string(),
                version: version.to_string(),
            });
        }
        if version == "latest" {
            matches.sort_by(|a, b| a.version.cmp(&b.version));
            return Ok(matches.pop().expect("non-empty"));
        }
        matches
            .into_iter()
            .find(|e| e.version == version)
            .ok_or_else(|| RegistryError::UnknownPlugin {
                id: id.to_string(),
                version: version.to_string(),
            })
    }

    /// The registry-root revocation list (`keys/revoked.toml`). Absent ⇒ empty.
    pub fn revocations(&self) -> Result<RevocationList, RegistryError> {
        let raw = match self.transport.fetch("keys/revoked.toml") {
            Ok(b) => b,
            Err(RegistryError::NotFound { .. }) => return Ok(RevocationList::default()),
            Err(e) => return Err(e),
        };
        let text = String::from_utf8(raw).map_err(|e| RegistryError::Parse {
            path: "keys/revoked.toml".into(),
            message: format!("not UTF-8: {e}"),
        })?;
        let parsed: RevokedFile = toml::from_str(&text).map_err(|e| RegistryError::Parse {
            path: "keys/revoked.toml".into(),
            message: e.to_string(),
        })?;
        Ok(RevocationList {
            revoked_keys: parsed.revoked_keys,
            revoked_plugins: parsed.revoked_plugins,
        })
    }

    fn version_path(id: &str, version: &str, file: &str) -> String {
        format!("plugins/{id}/{version}/{file}")
    }

    /// Resolve, download, and fully verify a plugin artifact.
    ///
    /// Verification order: (1) recompute the artifact's SHA-256 and match it
    /// against the index checksum; (2) run the Ed25519 + TOFU chain. Any
    /// mismatch rejects the artifact. `root_pubkeys` are the compile-pinned
    /// roots ([`crate::signing::pinned_root_pubkeys`]); `pinned_author` is the
    /// TOFU pin for this namespace, or `None` on first install.
    pub fn fetch_verified(
        &self,
        id: &str,
        version: &str,
        root_pubkeys: &[[u8; 32]],
        pinned_author: Option<[u8; 32]>,
    ) -> Result<VerifiedPlugin, RegistryError> {
        let entry = self.resolve(id, version)?;
        let id = entry.id.as_str();
        let version = entry.version.as_str();

        let manifest_bytes =
            self.transport
                .fetch(&Self::version_path(id, version, "manifest.yaml"))?;
        let entrypoint_bytes =
            self.transport
                .fetch(&Self::version_path(id, version, "entrypoint.wasm"))?;

        // (1) checksum / integrity guard.
        let actual = hex::encode(Sha256::digest(&entrypoint_bytes));
        let expected = entry.sha256.trim().to_lowercase();
        if actual != expected {
            return Err(RegistryError::ChecksumMismatch {
                id: id.to_string(),
                version: version.to_string(),
                expected,
                actual,
            });
        }

        let manifest = PluginManifest::load_from_yaml(&manifest_bytes).map_err(|e| {
            RegistryError::Malformed {
                id: id.to_string(),
                version: version.to_string(),
                message: format!("manifest: {e}"),
            }
        })?;

        // Signature + claim are required: this method does not install unsigned
        // plugins (that path lives behind an explicit `--allow-unsigned` flow).
        let signature =
            match self
                .transport
                .fetch(&Self::version_path(id, version, "signature.bin"))
            {
                Ok(b) => b,
                Err(RegistryError::NotFound { .. }) => {
                    return Err(RegistryError::Unsigned {
                        id: id.to_string(),
                        version: version.to_string(),
                    })
                }
                Err(e) => return Err(e),
            };
        let claim_raw = match self
            .transport
            .fetch(&Self::version_path(id, version, "claim.toml"))
        {
            Ok(b) => b,
            Err(RegistryError::NotFound { .. }) => {
                return Err(RegistryError::Unsigned {
                    id: id.to_string(),
                    version: version.to_string(),
                })
            }
            Err(e) => return Err(e),
        };

        let claim_file: ClaimFile = {
            let text = String::from_utf8(claim_raw).map_err(|e| RegistryError::Malformed {
                id: id.to_string(),
                version: version.to_string(),
                message: format!("claim.toml not UTF-8: {e}"),
            })?;
            toml::from_str(&text).map_err(|e| RegistryError::Malformed {
                id: id.to_string(),
                version: version.to_string(),
                message: format!("claim.toml: {e}"),
            })?
        };

        let author_pubkey =
            decode_key32(&claim_file.author_pubkey).map_err(|m| RegistryError::Malformed {
                id: id.to_string(),
                version: version.to_string(),
                message: format!("author_pubkey: {m}"),
            })?;
        let rotation_of = match &claim_file.rotation_of {
            None => None,
            Some(h) => Some(decode_key32(h).map_err(|m| RegistryError::Malformed {
                id: id.to_string(),
                version: version.to_string(),
                message: format!("rotation_of: {m}"),
            })?),
        };
        let claim_signature =
            hex::decode(claim_file.signature.trim()).map_err(|e| RegistryError::Malformed {
                id: id.to_string(),
                version: version.to_string(),
                message: format!("claim signature hex: {e}"),
            })?;

        let claim = NamespaceClaim {
            namespace: claim_file.namespace,
            keyid: claim_file.keyid,
            author_pubkey,
            rotation_of,
        };

        let signed = SignedPlugin {
            plugin_id: id,
            version,
            manifest_bytes: &manifest_bytes,
            entrypoint_bytes: &entrypoint_bytes,
            signature: &signature,
            claim: &claim,
            claim_signature: &claim_signature,
        };

        let revocations = self.revocations()?;
        let (status, pin) =
            verify_signed_plugin(&signed, root_pubkeys, &revocations, pinned_author).map_err(
                |source| RegistryError::Verification {
                    id: id.to_string(),
                    version: version.to_string(),
                    source,
                },
            )?;

        Ok(VerifiedPlugin {
            id: id.to_string(),
            version: version.to_string(),
            manifest,
            manifest_bytes,
            entrypoint_bytes,
            status,
            author_key_to_pin: pin,
        })
    }
}

fn decode_key32(hex_str: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(hex_str.trim()).map_err(|e| e.to_string())?;
    <[u8; 32]>::try_from(bytes.as_slice())
        .map_err(|_| format!("expected 32 bytes, got {}", bytes.len()))
}

#[cfg(test)]
mod tests;
