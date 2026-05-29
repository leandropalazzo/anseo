//! Builder that converts a [`PluginManifest`] into the shape consumed by
//! `crates/storage/src/repositories/plugin_installs.rs::NewPluginInstall`
//! (Story 0.12).
//!
//! ### Signing placeholder — read this before touching anything
//!
//! Story 17.1 ships **substrate only**. The real signing-verification path —
//! Ed25519 detached signature + namespace public key + maintainer-rotated root
//! key + TOFU pin — is intentionally deferred to a later story so it can land
//! under in-person review. The fields it would set on the audit row are
//! **hard-coded here** to:
//!
//! * `signature_verified = false`
//! * `signing_trust_root = "unsigned-substrate"` (see
//!   [`UNSIGNED_SUBSTRATE_TRUST_ROOT`])
//! * `publisher_pubkey_fingerprint = "unsigned-substrate"`
//!
//! When the signing story lands, [`NewInstallRecord::from_manifest`] will take
//! a `SigningResult` second argument and these constants disappear. Until
//! then, every audit row this builder produces carries the literal string
//! `"unsigned-substrate"` so a grep over the `plugin_installs` table reveals
//! exactly which rows pre-date the signing work.

use crate::manifest::PluginManifest;
use serde_json::json;

/// Literal trust-root label written into the `plugin_installs` audit table
/// for every install recorded by the substrate build. Searchable on purpose.
pub const UNSIGNED_SUBSTRATE_TRUST_ROOT: &str = "unsigned-substrate";

/// Owned mirror of `opengeo_storage::repositories::plugin_installs::NewPluginInstall`
/// so this crate does not link `crates/storage`. The CLI / install-runtime
/// story that wires this up converts field-for-field at call time.
///
/// The struct shape is deliberately the same field names + types as the
/// repo struct in Story 0.12, modulo the `'a` lifetimes (we hold owned
/// strings because the call site owns the manifest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstallRecord {
    pub plugin_name: String,
    pub plugin_version: String,
    pub publisher_pubkey_fingerprint: String,
    pub installed_by_actor: String,
    /// JSON array of capability strings (the canonical `tag` of each
    /// declared capability). Round-trip-safe with the storage repo's
    /// `capability_set: JsonValue` column.
    pub capability_set: serde_json::Value,
    pub signature_verified: bool,
    pub signing_trust_root: String,
}

impl NewInstallRecord {
    /// Build an audit-row payload from a validated manifest.
    ///
    /// The caller is responsible for having run [`PluginManifest::validate`]
    /// first; this builder is purely a shape-mapper.
    ///
    /// `installed_by_actor` is the CLI's caller identity (the
    /// `--actor` flag or the current OS user); the substrate just passes it
    /// through.
    ///
    /// **Signing placeholder:** see module docs. This call always emits
    /// `signature_verified = false` and `signing_trust_root = "unsigned-substrate"`.
    pub fn from_manifest(manifest: &PluginManifest, installed_by_actor: &str) -> Self {
        let capability_tags: Vec<String> = manifest
            .capabilities
            .iter()
            .map(|c| c.tag().to_string())
            .collect();

        Self {
            plugin_name: manifest.name.clone(),
            plugin_version: manifest.version.clone(),
            publisher_pubkey_fingerprint: UNSIGNED_SUBSTRATE_TRUST_ROOT.to_string(),
            installed_by_actor: installed_by_actor.to_string(),
            capability_set: json!(capability_tags),
            signature_verified: false,
            signing_trust_root: UNSIGNED_SUBSTRATE_TRUST_ROOT.to_string(),
        }
    }
}
