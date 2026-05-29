//! `ogeo plugin validate <path>` — Story 17.1 SUBSTRATE ONLY.
//!
//! Loads a plugin manifest YAML from a path, runs the pure-data
//! [`PluginManifest::validate`] pass, and prints results.
//!
//! **No install command in this story.** The install runtime — registry
//! resolution, signature verification, on-disk layout, and WASM host
//! instantiation — is intentionally deferred for in-person review. This
//! command is the read-only front edge of the SDK so plugin authors can
//! check their manifests today.

use std::path::PathBuf;

use clap::Args;
use opengeo_core::OpenGeoError;
use opengeo_plugin_manifest::PluginManifest;

#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// Path to a plugin manifest YAML (typically `plugin.yaml`).
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
}

pub fn run_validate(args: ValidateArgs) -> Result<(), OpenGeoError> {
    let manifest = PluginManifest::load_from_path(&args.path)
        .map_err(|e| OpenGeoError::Config(format!("failed to load manifest: {e}")))?;

    match manifest.validate() {
        Ok(()) => {
            println!(
                "OK  {} v{} ({}) — {} capability declaration(s)",
                manifest.name,
                manifest.version,
                manifest.plugin_type,
                manifest.capabilities.len()
            );
            println!("note: substrate-only validation. No signing check. No host load.");
            Ok(())
        }
        Err(errs) => {
            eprintln!("manifest at {} has {} error(s):", args.path.display(), errs.len());
            for (i, e) in errs.iter().enumerate() {
                eprintln!("  {}. {e}", i + 1);
            }
            Err(OpenGeoError::Config(format!(
                "manifest validation failed ({} error(s))",
                errs.len()
            )))
        }
    }
}
