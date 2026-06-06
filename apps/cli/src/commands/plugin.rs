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
use std::time::{Duration, SystemTime};

use anseo_core::OpenGeoError;
use anseo_plugin_host::registry::{
    HttpTransport, InMemoryTransport, RegistryClient, RegistryError, RegistryTransport,
    DEFAULT_REGISTRY_URL, REGISTRY_URL_ENV,
};
use anseo_plugin_host::signing::pinned_root_pubkeys;
use anseo_plugin_manifest::PluginManifest;
use anseo_storage::Storage;
use clap::Args;

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
/// `ANSEO_PLUGIN_REGISTRY`, else the local cache under the plugin home. The
/// GitHub-backed transport syncs that cache; this story ships the resolver and
/// install pipeline over the on-disk tree.
fn resolve_registry(flag: Option<&std::path::Path>) -> Result<FsRegistry, OpenGeoError> {
    if let Some(p) = flag {
        return Ok(FsRegistry::new(p));
    }
    if let Ok(p) = std::env::var("ANSEO_PLUGIN_REGISTRY") {
        return Ok(FsRegistry::new(p));
    }
    Err(OpenGeoError::Config(
        "no plugin registry configured; pass --registry <dir> or set ANSEO_PLUGIN_REGISTRY \
         (GitHub-backed registry sync lands with the transport layer)"
            .into(),
    ))
}

/// `~/.config/opengeo` (XDG), overridable via `ANSEO_PLUGIN_HOME` for tests.
fn plugin_home() -> Result<PathBuf, OpenGeoError> {
    if let Ok(p) = std::env::var("ANSEO_PLUGIN_HOME") {
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

/// How long a fetched `index.toml` stays fresh before `search` re-fetches it.
/// Raw GitHub content is CDN-served; a 1-hour TTL keeps discovery snappy
/// without hammering the CDN (Story 41.1 Notes). Bust it with `--refresh`.
const INDEX_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Substring matched against plugin id + description.
    pub query: String,
    /// Search a **local** registry directory instead of the live GitHub
    /// registry (offline / development use).
    #[arg(long, value_name = "DIR")]
    pub registry: Option<PathBuf>,
    /// Bypass the 1-hour `index.toml` cache and re-fetch from the registry.
    #[arg(long)]
    pub refresh: bool,
}

pub fn run_search(args: SearchArgs) -> Result<(), OpenGeoError> {
    // Offline / dev path: an explicit local registry dir wins.
    if let Some(dir) = args.registry.as_deref() {
        let registry = FsRegistry::new(dir);
        let hits = registry
            .search(&args.query)
            .map_err(|e| OpenGeoError::Config(e.to_string()))?;
        print_search_results(
            &args.query,
            hits.into_iter().map(|e| (e.id, e.version, e.description)),
        );
        return Ok(());
    }

    // Default path: query the live GitHub flat-file registry (Story 41.1).
    let client = live_registry_client(args.refresh)?;
    let hits = match client.search_lenient(&args.query) {
        Ok(hits) => hits,
        Err(RegistryError::Transport { .. } | RegistryError::NotFound { .. }) => {
            return Err(OpenGeoError::Config(
                "registry unreachable — try again later".into(),
            ));
        }
        Err(e) => return Err(OpenGeoError::Config(e.to_string())),
    };
    print_search_results(
        &args.query,
        hits.into_iter().map(|e| (e.id, e.version, e.description)),
    );
    Ok(())
}

fn print_search_results(query: &str, hits: impl Iterator<Item = (String, String, String)>) {
    let mut any = false;
    for (id, version, description) in hits {
        any = true;
        println!("{id}@{version}  —  {description}");
    }
    if !any {
        println!("no plugins match `{query}`");
        println!(
            "the Anseo plugin registry is community-seeded and may be empty; \
             results are cached for up to 1h — pass --refresh to re-check."
        );
    }
}

/// Build a registry client over a 1-hour-cached snapshot of the live registry
/// `index.toml`. On a cache hit we serve the cached bytes through an
/// [`InMemoryTransport`]; on a miss (or `refresh`) we fetch the live index and
/// persist it. A network failure with no usable cache surfaces as a transport
/// error so the caller can render the "registry unreachable" message.
fn live_registry_client(refresh: bool) -> Result<RegistryClient<InMemoryTransport>, OpenGeoError> {
    let base_url = resolved_registry_base_url();
    let cache_path = index_cache_path(&base_url)?;
    let fresh_cache = !refresh
        && cache_path
            .metadata()
            .and_then(|m| m.modified())
            .map(|modified| {
                SystemTime::now()
                    .duration_since(modified)
                    .map(|age| age < INDEX_CACHE_TTL)
                    .unwrap_or(false)
            })
            .unwrap_or(false);

    let index_bytes: Vec<u8> = if fresh_cache {
        std::fs::read(&cache_path).map_err(|e| {
            OpenGeoError::Config(format!("read registry cache {}: {e}", cache_path.display()))
        })?
    } else {
        let transport = HttpTransport::from_env();
        match transport.fetch("index.toml") {
            Ok(bytes) => {
                // Best-effort cache write; failure to cache is non-fatal.
                if let Some(parent) = cache_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&cache_path, &bytes);
                bytes
            }
            // Network/transport failure: fall back to a stale cache if present.
            Err(RegistryError::Transport { .. } | RegistryError::NotFound { .. }) => {
                match std::fs::read(&cache_path) {
                    Ok(stale) => {
                        eprintln!(
                            "warning: registry unreachable — showing cached results \
                             (pass --refresh to retry)"
                        );
                        stale
                    }
                    Err(_) => {
                        return Err(OpenGeoError::Config(
                            "registry unreachable — try again later".into(),
                        ))
                    }
                }
            }
            Err(e) => return Err(OpenGeoError::Config(e.to_string())),
        }
    };

    let mem = InMemoryTransport::new();
    mem.insert("index.toml", index_bytes);
    Ok(RegistryClient::new(mem))
}

/// Resolve the registry base URL exactly as [`HttpTransport::from_env`] does:
/// `ANSEO_PLUGIN_REGISTRY_URL`, then the deprecated `OPENGEO_PLUGIN_REGISTRY_URL`,
/// then [`DEFAULT_REGISTRY_URL`]. The trailing slash is trimmed so that
/// `https://r/` and `https://r` resolve to the same cache key.
fn resolved_registry_base_url() -> String {
    std::env::var(REGISTRY_URL_ENV)
        .or_else(|_| std::env::var("OPENGEO_PLUGIN_REGISTRY_URL"))
        .unwrap_or_else(|_| DEFAULT_REGISTRY_URL.into())
        .trim_end_matches('/')
        .to_string()
}

/// `<plugin_home>/cache/index-<hash(base_url)>.toml`.
///
/// The cache filename is keyed by a stable hash of the resolved registry base
/// URL so that switching `ANSEO_PLUGIN_REGISTRY_URL` (e.g. to a fork or an
/// internal registry) never serves the previous registry's stale index. The
/// default registry keeps a stable key; each override gets its own. A non-crypto
/// hash is sufficient — this is only a filesystem cache key, not a security
/// boundary.
fn index_cache_path(base_url: &str) -> Result<PathBuf, OpenGeoError> {
    let key = index_cache_key(base_url);
    Ok(plugin_home()?
        .join("cache")
        .join(format!("index-{key}.toml")))
}

/// Stable 16-hex-char key derived from the registry base URL.
fn index_cache_key(base_url: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    base_url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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

#[cfg(test)]
mod cache_key_tests {
    use super::{index_cache_key, resolved_registry_base_url, DEFAULT_REGISTRY_URL};

    #[test]
    fn same_url_is_stable() {
        let url = "https://example.test/registry";
        assert_eq!(index_cache_key(url), index_cache_key(url));
    }

    #[test]
    fn different_urls_map_to_different_keys() {
        let default = index_cache_key(DEFAULT_REGISTRY_URL);
        let fork = index_cache_key("https://raw.githubusercontent.com/acme/plugin-registry/main");
        let internal = index_cache_key("https://registry.internal.example/plugins");
        assert_ne!(default, fork, "fork must not reuse the default cache key");
        assert_ne!(default, internal);
        assert_ne!(fork, internal);
    }

    #[test]
    fn key_is_filename_safe_16_hex() {
        let key = index_cache_key("https://example.test/x?y=z&a=b/../weird");
        assert_eq!(key.len(), 16);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn trailing_slash_does_not_change_resolved_url() {
        // env access is process-global; serialize via a single test that sets and
        // clears the override so it doesn't leak into other tests.
        std::env::set_var(super::REGISTRY_URL_ENV, "https://example.test/reg/");
        assert_eq!(resolved_registry_base_url(), "https://example.test/reg");
        std::env::set_var(super::REGISTRY_URL_ENV, "https://example.test/reg");
        assert_eq!(resolved_registry_base_url(), "https://example.test/reg");
        std::env::remove_var(super::REGISTRY_URL_ENV);
        assert_eq!(resolved_registry_base_url(), DEFAULT_REGISTRY_URL);
    }
}
