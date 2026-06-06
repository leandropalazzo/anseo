//! [`PluginManifest`] — on-disk YAML shape per arch §2.5 / §3.2.
//!
//! Story 17.1 ships the data shape and a load-from-path helper. Resolution,
//! signing verification, and host instantiation are later stories.

use crate::capability::Capability;
use crate::plugin_type::PluginType;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// The `publisher` value that marks a plugin as first-party (Story 41.4). A
/// first-party plugin must be signed; install hard-errors if its signature is
/// missing or invalid, ignoring `--allow-unsigned`.
pub const FIRST_PARTY_PUBLISHER: &str = "anseo.ai";

/// Plugin manifest as declared in `plugin.yaml` on disk.
///
/// Arch §2.5 names the file `plugin.toml`; we ship YAML here per the brief
/// (the brief says "arch suggests YAML — match that"). The on-disk format is
/// the only thing this struct describes; the wire-level audit-row format is
/// [`crate::install_record::NewInstallRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Optional publisher identity (Story 41.4). When this equals
    /// [`FIRST_PARTY_PUBLISHER`] the plugin is treated as **first-party** and a
    /// valid signature is *required* at install time (no `--allow-unsigned`
    /// escape hatch); community plugins (any other / empty value) may install
    /// unsigned with a warning.
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub homepage: String,
    /// Closed catalog; see [`Capability`].
    pub capabilities: Vec<Capability>,
    /// Discriminator across the four plugin kinds.
    pub plugin_type: PluginType,
    /// Relative path inside the plugin bundle that points at the WASM module
    /// (or subprocess binary for analytics plugins). Validation forbids
    /// absolute paths and `..` traversal.
    pub entry_point: PathBuf,
}

#[derive(Debug, Error)]
pub enum ManifestLoadError {
    #[error("failed to read manifest at `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse manifest at `{path}`: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("failed to parse manifest bytes: {source}")]
    ParseBytes {
        #[source]
        source: serde_yaml::Error,
    },
}

impl PluginManifest {
    /// Load a manifest from disk. Performs the strict-parse pass but does
    /// **not** run [`Self::validate`] — callers run that separately so a
    /// `validate`-only CLI can distinguish "won't parse" from "parses but
    /// has logical errors."
    pub fn load_from_path(path: &Path) -> Result<Self, ManifestLoadError> {
        let bytes = std::fs::read(path).map_err(|source| ManifestLoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let manifest =
            serde_yaml::from_slice::<Self>(&bytes).map_err(|source| ManifestLoadError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(manifest)
    }

    /// Whether this manifest declares a first-party publisher (Story 41.4).
    /// First-party plugins are signature-required at install time.
    pub fn is_first_party(&self) -> bool {
        self.publisher == FIRST_PARTY_PUBLISHER
    }

    /// Strict-parse a manifest from in-memory YAML bytes. Same parse pass as
    /// [`Self::load_from_path`] but for callers that already hold the bytes
    /// (e.g. a registry client that fetched them over HTTP). Does **not** run
    /// [`Self::validate`].
    pub fn load_from_yaml(bytes: &[u8]) -> Result<Self, ManifestLoadError> {
        serde_yaml::from_slice::<Self>(bytes)
            .map_err(|source| ManifestLoadError::ParseBytes { source })
    }
}
