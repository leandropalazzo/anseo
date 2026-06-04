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
use opengeo_plugin_host::signing::pinned_root_pubkeys;
use opengeo_plugin_manifest::PluginManifest;
use opengeo_storage::Storage;

use super::plugin_install::{
    install_plugin, list_installed, remove_plugin, upgrade_plugin, InstallOptions,
};
use super::plugin_registry::FsRegistry;

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
            eprintln!(
                "manifest at {} has {} error(s):",
                args.path.display(),
                errs.len()
            );
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

// ---------------------------------------------------------------------------
// Story 17.5 — registry + install runtime.
// ---------------------------------------------------------------------------

/// Resolve the registry root: `--registry` flag wins, else
/// `OPENGEO_PLUGIN_REGISTRY`, else the local cache under the plugin home. The
/// GitHub-backed transport syncs that cache; this story ships the resolver and
/// install pipeline over the on-disk tree.
fn resolve_registry(flag: Option<&std::path::Path>) -> Result<FsRegistry, OpenGeoError> {
    if let Some(p) = flag {
        return Ok(FsRegistry::new(p));
    }
    if let Ok(p) = std::env::var("OPENGEO_PLUGIN_REGISTRY") {
        return Ok(FsRegistry::new(p));
    }
    Err(OpenGeoError::Config(
        "no plugin registry configured; pass --registry <dir> or set OPENGEO_PLUGIN_REGISTRY \
         (GitHub-backed registry sync lands with the transport layer)"
            .into(),
    ))
}

/// `~/.config/opengeo` (XDG), overridable via `OPENGEO_PLUGIN_HOME` for tests.
fn plugin_home() -> Result<PathBuf, OpenGeoError> {
    if let Ok(p) = std::env::var("OPENGEO_PLUGIN_HOME") {
        return Ok(PathBuf::from(p));
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map_err(|_| OpenGeoError::Config("cannot determine config home (set HOME)".into()))?;
    Ok(base.join("opengeo"))
}

async fn open_pool() -> Result<Storage, OpenGeoError> {
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| OpenGeoError::Config("DATABASE_URL must be set for plugin install".into()))?;
    Storage::connect(&url)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to connect to Postgres: {e}")))
}

/// Parse `namespace/name@version` (version optional → `latest`).
fn parse_spec(spec: &str) -> (String, String) {
    match spec.split_once('@') {
        Some((id, ver)) => (id.to_string(), ver.to_string()),
        None => (spec.to_string(), "latest".to_string()),
    }
}

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Substring matched against plugin id + description.
    pub query: String,
    #[arg(long, value_name = "DIR")]
    pub registry: Option<PathBuf>,
}

pub fn run_search(args: SearchArgs) -> Result<(), OpenGeoError> {
    let registry = resolve_registry(args.registry.as_deref())?;
    let hits = registry
        .search(&args.query)
        .map_err(|e| OpenGeoError::Config(e.to_string()))?;
    if hits.is_empty() {
        println!("no plugins match `{}`", args.query);
        return Ok(());
    }
    for e in hits {
        println!("{}@{}  —  {}", e.id, e.version, e.description);
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// `namespace/name[@version]` (default version: latest).
    pub spec: String,
    #[arg(long, value_name = "DIR")]
    pub registry: Option<PathBuf>,
    /// Install an unsigned plugin (records signature_status=unsigned).
    #[arg(long)]
    pub allow_unsigned: bool,
}

pub async fn run_install(args: InstallArgs) -> Result<(), OpenGeoError> {
    let registry = resolve_registry(args.registry.as_deref())?;
    let home = plugin_home()?;
    let storage = open_pool().await?;
    let (id, version) = parse_spec(&args.spec);
    let opts = InstallOptions {
        allow_unsigned: args.allow_unsigned,
        actor: std::env::var("USER").unwrap_or_else(|_| "local".into()),
        ..Default::default()
    };
    let outcome = install_plugin(
        storage.pool(),
        &registry,
        &home,
        &id,
        &version,
        &opts,
        &pinned_root_pubkeys(),
    )
    .await
    .map_err(|e| OpenGeoError::Config(e.to_string()))?;
    println!(
        "installed {}@{} ({}) → {}",
        outcome.id,
        outcome.version,
        outcome.signature_status.as_str(),
        outcome.install_dir.display()
    );
    println!(
        "restart your worker (e.g. docker compose restart worker) for the plugin to take effect"
    );
    Ok(())
}

#[derive(Debug, Args)]
pub struct ListArgs {}

pub async fn run_list(_args: ListArgs) -> Result<(), OpenGeoError> {
    let storage = open_pool().await?;
    let rows = list_installed(storage.pool())
        .await
        .map_err(|e| OpenGeoError::Config(e.to_string()))?;
    if rows.is_empty() {
        println!("no plugins installed");
        return Ok(());
    }
    for r in rows {
        let flag = if r.signature_verified {
            "signed"
        } else {
            &r.signing_trust_root
        };
        println!("{}@{}  [{}]", r.plugin_name, r.plugin_version, flag);
    }
    Ok(())
}

#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Plugin id to remove.
    pub id: String,
}

pub async fn run_remove(args: RemoveArgs) -> Result<(), OpenGeoError> {
    let home = plugin_home()?;
    let storage = open_pool().await?;
    remove_plugin(storage.pool(), &home, &args.id)
        .await
        .map_err(|e| OpenGeoError::Config(e.to_string()))?;
    println!("removed {}", args.id);
    Ok(())
}

#[derive(Debug, Args)]
pub struct UpgradeArgs {
    /// `namespace/name[@version]` (default version: latest).
    pub spec: String,
    #[arg(long, value_name = "DIR")]
    pub registry: Option<PathBuf>,
    #[arg(long)]
    pub allow_unsigned: bool,
    /// Accept a capability-set widening (§6.4 breaking upgrade).
    #[arg(long)]
    pub accept_new_capabilities: bool,
}

pub async fn run_upgrade(args: UpgradeArgs) -> Result<(), OpenGeoError> {
    let registry = resolve_registry(args.registry.as_deref())?;
    let home = plugin_home()?;
    let storage = open_pool().await?;
    let (id, version) = parse_spec(&args.spec);
    let opts = InstallOptions {
        allow_unsigned: args.allow_unsigned,
        accept_new_capabilities: args.accept_new_capabilities,
        actor: std::env::var("USER").unwrap_or_else(|_| "local".into()),
    };
    let outcome = upgrade_plugin(
        storage.pool(),
        &registry,
        &home,
        &id,
        &version,
        &opts,
        &pinned_root_pubkeys(),
    )
    .await
    .map_err(|e| OpenGeoError::Config(e.to_string()))?;
    println!("upgraded {}@{}", outcome.id, outcome.version);
    Ok(())
}
