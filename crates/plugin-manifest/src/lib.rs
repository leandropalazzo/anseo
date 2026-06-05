//! `anseo-plugin-manifest` — Story 17.1 SUBSTRATE ONLY.
//!
//! This crate ships **only** the static surface of the Phase 3 Plugin SDK:
//!
//! * the [`PluginManifest`] struct (parsed from YAML on disk),
//! * the closed [`Capability`] catalog (strict-parse, unknown strings rejected),
//! * the [`PluginType`] discriminator,
//! * a [`PluginManifest::validate`] pass over fields that don't require I/O,
//! * a [`NewInstallRecord`] builder that maps a manifest to the shape consumed
//!   by `crates/storage/src/repositories/plugin_installs.rs` (Story 0.12).
//!
//! ### Out of scope for this crate (intentionally)
//!
//! * The WASM host (`wasmtime`, wit-bindgen, capability enforcement at call
//!   time) — security-sensitive, lands in a later story.
//! * Signing and the trust root (Ed25519 verification, TOFU registry,
//!   revocation list) — security-sensitive, lands in a later story.
//! * The `ogeo plugin install` runtime, registry resolver, and on-disk plugin
//!   directory layout — lands in a later story.
//!
//! No dependency on `wasmtime`, `wit-bindgen`, `ed25519-dalek`, `sigstore`,
//! `cosign`, or any subprocess-sandbox crate appears in `Cargo.toml`. This
//! constraint is part of the Story 17.1 scope contract.

pub mod capability;
pub mod install_record;
pub mod manifest;
pub mod plugin_type;
pub mod trend_kind;
pub mod validation;

pub use capability::{Capability, CapabilityParseError};
pub use install_record::{NewInstallRecord, UNSIGNED_SUBSTRATE_TRUST_ROOT};
pub use manifest::{ManifestLoadError, PluginManifest};
pub use plugin_type::{PluginType, PluginTypeParseError};
pub use validation::ValidationError;

#[cfg(test)]
mod tests;
