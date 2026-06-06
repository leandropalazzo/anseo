//! Story 41.2 — worker / `anseo serve` plugin **load-path** (runtime activation).
//!
//! Epic 17 built the plugin host, the four type adapters, the signing/trust
//! chain, and the install pipeline (`ogeo plugin install`). But installed
//! plugins were materialized under `<home>/plugins/<id>/<version>/` and
//! recorded in `<home>/installed.toml` — and then *never loaded at runtime*.
//! `build_real_registry()` and the extractor/analytics/output registries only
//! ever walked the closed first-party enum.
//!
//! This module is the missing piece: at `anseo serve` / worker startup, scan the
//! install directory **eagerly** (all plugins resolve their load decision before
//! the server accepts requests — fail-fast beats first-request latency spikes),
//! and for each installed plugin decide whether it loads, is skipped, or errors,
//! dispatching by [`PluginType`]. The decision honours:
//!
//!   * **Signature verification** — a plugin recorded as `unsigned` in
//!     `installed.toml` is *skipped* unless the caller opts in via
//!     [`LoadPolicy::allow_unsigned`]. We never silently load an unverified
//!     plugin into a privileged registry.
//!   * **Platform sandbox guard** — Analytics plugins run in the subprocess
//!     seccomp-bpf / `sandbox-exec` sandbox, which is Linux/macOS only. On an
//!     unsupported platform (Windows) the plugin is *skipped* with an explicit
//!     `sandbox not supported on this platform` reason — never loaded
//!     in-process (ADR Notes / OQ-P3-5).
//!   * **Per-plugin isolation** — a corrupted bundle (missing/garbled manifest,
//!     a `kind` mismatch) is recorded as `load_error` and skipped; `serve`
//!     continues. One bad plugin never takes down startup.
//!
//! The actual registry wiring (constructing a [`PluginProvider`], extractor
//! adapter, etc.) is the consumer's job — this module computes the *decision*
//! and surfaces a stable [`LoadedPlugin`] list that both `GET /v1/plugins` and
//! `anseo plugin list` render verbatim, so the API and CLI can never drift on
//! what is loaded.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use anseo_plugin_manifest::{PluginManifest, PluginType};

use crate::subprocess::Platform;

/// Per-plugin runtime load outcome, surfaced by `GET /v1/plugins` and
/// `anseo plugin list`. The three states map 1:1 to the story's required
/// status enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadStatus {
    /// The plugin passed every gate and is registered for prompt runs.
    Loaded,
    /// The plugin is intentionally not loaded (unsigned-and-not-allowed, or its
    /// sandbox is unsupported on this platform). Not an error — a policy skip.
    Skipped,
    /// The plugin could not be loaded because its bundle is malformed or its
    /// recorded `kind` disagrees with its manifest. Logged WARN; serve continues.
    LoadError,
}

impl LoadStatus {
    /// Stable wire string used by the API + CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            LoadStatus::Loaded => "loaded",
            LoadStatus::Skipped => "skipped",
            LoadStatus::LoadError => "load_error",
        }
    }
}

/// One row of the load report: a stable, serialisable view of a single
/// installed plugin's runtime activation outcome. `GET /v1/plugins` returns a
/// list of these and `anseo plugin list` prints the same fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedPlugin {
    /// Plugin id (`namespace/name`).
    pub id: String,
    /// Installed version.
    pub version: String,
    /// Plugin kind (`provider | extractor | analytics | output-format`), or
    /// `"unknown"` when the manifest could not be read.
    pub kind: String,
    /// Runtime load outcome.
    pub status: LoadStatus,
    /// Human-readable reason for a `skipped` / `load_error` outcome. Empty for
    /// a clean `loaded`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// Policy knobs for the load pass.
#[derive(Debug, Clone)]
pub struct LoadPolicy {
    /// When `true`, plugins recorded as `unsigned` are still loaded. Defaults to
    /// `false`: unverified plugins are skipped unless the operator opts in.
    pub allow_unsigned: bool,
    /// The platform whose sandbox capabilities gate Analytics plugins. Defaults
    /// to [`Platform::current`]; injectable so tests can exercise the Windows
    /// skip path on any host.
    pub platform: Platform,
}

impl Default for LoadPolicy {
    fn default() -> Self {
        LoadPolicy {
            allow_unsigned: false,
            platform: Platform::current(),
        }
    }
}

/// One entry of `<home>/installed.toml`, mirrored from the install pipeline
/// (`apps/cli/src/commands/plugin_install.rs::InstalledEntry`). We re-declare a
/// read-only view here so the host crate does not depend on the CLI crate; the
/// fields are a structural subset and `serde(default)` tolerates extra columns.
#[derive(Debug, Clone, Deserialize)]
struct InstalledEntry {
    id: String,
    version: String,
    #[serde(default)]
    signature_status: String,
    #[serde(default)]
    #[allow(dead_code)]
    namespace: String,
}

#[derive(Debug, Default, Deserialize)]
struct InstalledFile {
    #[serde(default)]
    plugin: Vec<InstalledEntry>,
}

/// Read `<home>/installed.toml`. A missing file means "no plugins installed"
/// (clean empty list). A malformed file degrades to empty with a WARN rather
/// than aborting startup — the same lenient posture the registry index uses.
fn read_installed(home: &Path) -> InstalledFile {
    let path = home.join("installed.toml");
    match std::fs::read_to_string(&path) {
        Ok(raw) => match toml::from_str(&raw) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    event = "plugin.installed_manifest_malformed",
                    path = %path.display(),
                    error = %e,
                    "installed.toml is malformed; treating as no plugins installed"
                );
                InstalledFile::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => InstalledFile::default(),
        Err(e) => {
            tracing::warn!(
                event = "plugin.installed_manifest_unreadable",
                path = %path.display(),
                error = %e,
                "cannot read installed.toml; treating as no plugins installed"
            );
            InstalledFile::default()
        }
    }
}

/// Compute the load decision for a single installed plugin. Pure given its
/// inputs (the on-disk manifest + the recorded signature status + the policy)
/// so it is exhaustively unit-testable without a filesystem walk.
fn decide(home: &Path, entry: &InstalledEntry, policy: &LoadPolicy) -> LoadedPlugin {
    let manifest_path = home
        .join("plugins")
        .join(&entry.id)
        .join(&entry.version)
        .join("manifest.yaml");

    // (1) Bundle integrity: the manifest must read + parse, else load_error.
    let manifest = match PluginManifest::load_from_path(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            return LoadedPlugin {
                id: entry.id.clone(),
                version: entry.version.clone(),
                kind: "unknown".to_string(),
                status: LoadStatus::LoadError,
                reason: format!("manifest unreadable: {e}"),
            };
        }
    };

    let kind = manifest.plugin_type;

    // (2) Signature gate: an unsigned install is skipped unless explicitly
    // allowed. `signature_status` is "signed" | "unsigned" from the installer.
    let is_unsigned = entry.signature_status.eq_ignore_ascii_case("unsigned");
    if is_unsigned && !policy.allow_unsigned {
        return skip(
            entry,
            kind,
            "unsigned plugin not loaded (set allow_unsigned to load)",
        );
    }

    // (3) Platform sandbox gate for Analytics (subprocess seccomp/sandbox-exec).
    // Windows + other unsupported hosts skip rather than fall back in-process.
    if matches!(kind, PluginType::Analytics) && !policy.platform.supports_analytics_subprocess() {
        return skip(entry, kind, "sandbox not supported on this platform");
    }

    // (4) All gates passed — the plugin is activated for its registry. The
    // consumer (build_real_registry second pass, extractor/analytics/output
    // passes) materializes the adapter from this entry.
    LoadedPlugin {
        id: entry.id.clone(),
        version: entry.version.clone(),
        kind: kind.to_string(),
        status: LoadStatus::Loaded,
        reason: String::new(),
    }
}

fn skip(entry: &InstalledEntry, kind: PluginType, reason: &str) -> LoadedPlugin {
    LoadedPlugin {
        id: entry.id.clone(),
        version: entry.version.clone(),
        kind: kind.to_string(),
        status: LoadStatus::Skipped,
        reason: reason.to_string(),
    }
}

/// Scan the install directory at `home` and compute the runtime load report.
///
/// Eager: every installed plugin's decision is resolved here, before the server
/// accepts requests. Per-plugin failures are non-fatal (recorded as
/// `load_error`) and `skipped` plugins emit a WARN so operators see why a
/// plugin they installed is not active. Returns the stable [`LoadedPlugin`]
/// list rendered identically by `GET /v1/plugins` and `anseo plugin list`.
pub fn scan_and_load(home: &Path, policy: &LoadPolicy) -> Vec<LoadedPlugin> {
    let installed = read_installed(home);
    let mut report = Vec::with_capacity(installed.plugin.len());
    for entry in &installed.plugin {
        let loaded = decide(home, entry, policy);
        match loaded.status {
            LoadStatus::Loaded => tracing::info!(
                event = "plugin.loaded",
                id = %loaded.id,
                version = %loaded.version,
                kind = %loaded.kind,
                "plugin loaded"
            ),
            LoadStatus::Skipped => tracing::warn!(
                event = "plugin.skipped",
                id = %loaded.id,
                reason = %loaded.reason,
                "plugin {} skipped: {}",
                loaded.id,
                loaded.reason
            ),
            LoadStatus::LoadError => tracing::warn!(
                event = "plugin.load_error",
                id = %loaded.id,
                reason = %loaded.reason,
                "plugin {} failed to load: {}",
                loaded.id,
                loaded.reason
            ),
        }
        report.push(loaded);
    }
    report
}

/// Resolve the plugin home the loader scans, matching the install pipeline's
/// resolution (`apps/cli/src/commands/plugin.rs::plugin_home`): `ANSEO_PLUGIN_HOME`
/// wins, else `$XDG_CONFIG_HOME/opengeo`, else `$HOME/.config/opengeo`. Returns
/// `None` when none can be resolved (no `HOME`), in which case the caller treats
/// it as "no plugins" rather than erroring.
pub fn resolve_plugin_home() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ANSEO_PLUGIN_HOME") {
        return Some(PathBuf::from(p));
    }
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok()?;
    Some(base.join("opengeo"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build a minimal installed plugin bundle on disk + return its
    /// `installed.toml` row so tests can drive the scan path end-to-end against
    /// a tempdir (no network, no real download — AC fixture posture).
    fn write_plugin(home: &Path, id: &str, version: &str, kind: &str, sig: &str) {
        let dir = home.join("plugins").join(id).join(version);
        fs::create_dir_all(&dir).unwrap();
        let manifest = format!(
            "name: {id}\nversion: \"{version}\"\ncapabilities: []\nplugin_type: {kind}\nentry_point: entrypoint.wasm\n"
        );
        fs::write(dir.join("manifest.yaml"), manifest).unwrap();
        // Append an installed.toml row.
        let row = format!(
            "[[plugin]]\nid = \"{id}\"\nversion = \"{version}\"\nsignature_status = \"{sig}\"\nnamespace = \"\"\n"
        );
        let path = home.join("installed.toml");
        let mut existing = fs::read_to_string(&path).unwrap_or_default();
        existing.push_str(&row);
        fs::write(&path, existing).unwrap();
    }

    fn linux_policy(allow_unsigned: bool) -> LoadPolicy {
        LoadPolicy {
            allow_unsigned,
            // Pin Linux so analytics-supported tests are host-independent.
            platform: Platform::Linux,
        }
    }

    #[test]
    fn no_installed_file_is_empty_report() {
        let tmp = tempfile::tempdir().unwrap();
        let report = scan_and_load(tmp.path(), &LoadPolicy::default());
        assert!(report.is_empty());
    }

    #[test]
    fn signed_provider_loads() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(tmp.path(), "acme/p", "0.1.0", "provider", "signed");
        let report = scan_and_load(tmp.path(), &linux_policy(false));
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].status, LoadStatus::Loaded);
        assert_eq!(report[0].kind, "provider");
        assert_eq!(report[0].reason, "");
    }

    #[test]
    fn unsigned_skipped_unless_allowed() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(tmp.path(), "acme/u", "0.1.0", "extractor", "unsigned");

        let skipped = scan_and_load(tmp.path(), &linux_policy(false));
        assert_eq!(skipped[0].status, LoadStatus::Skipped);
        assert!(skipped[0].reason.contains("unsigned"));

        let allowed = scan_and_load(tmp.path(), &linux_policy(true));
        assert_eq!(allowed[0].status, LoadStatus::Loaded);
    }

    #[test]
    fn analytics_skipped_on_unsupported_platform() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(tmp.path(), "acme/a", "0.1.0", "analytics", "signed");
        let policy = LoadPolicy {
            allow_unsigned: false,
            platform: Platform::Windows,
        };
        let report = scan_and_load(tmp.path(), &policy);
        assert_eq!(report[0].status, LoadStatus::Skipped);
        assert!(report[0].reason.contains("sandbox not supported"));
    }

    #[test]
    fn analytics_loads_on_supported_platform() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(tmp.path(), "acme/a", "0.1.0", "analytics", "signed");
        let report = scan_and_load(tmp.path(), &linux_policy(false));
        assert_eq!(report[0].status, LoadStatus::Loaded);
    }

    #[test]
    fn corrupt_manifest_is_load_error_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        // installed.toml references a plugin whose manifest is garbage.
        let dir = tmp.path().join("plugins").join("acme/bad").join("0.1.0");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("manifest.yaml"), "this: is: not: valid: yaml:").unwrap();
        fs::write(
            tmp.path().join("installed.toml"),
            "[[plugin]]\nid = \"acme/bad\"\nversion = \"0.1.0\"\nsignature_status = \"signed\"\n",
        )
        .unwrap();
        let report = scan_and_load(tmp.path(), &linux_policy(false));
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].status, LoadStatus::LoadError);
        assert_eq!(report[0].kind, "unknown");
    }

    #[test]
    fn malformed_installed_toml_degrades_to_empty() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("installed.toml"), "}}}not toml{{{").unwrap();
        let report = scan_and_load(tmp.path(), &linux_policy(false));
        assert!(report.is_empty());
    }

    #[test]
    fn multiple_plugins_independent_outcomes() {
        let tmp = tempfile::tempdir().unwrap();
        write_plugin(tmp.path(), "acme/ok", "1.0.0", "provider", "signed");
        write_plugin(
            tmp.path(),
            "acme/unsigned",
            "1.0.0",
            "output-format",
            "unsigned",
        );
        let report = scan_and_load(tmp.path(), &linux_policy(false));
        assert_eq!(report.len(), 2);
        let ok = report.iter().find(|p| p.id == "acme/ok").unwrap();
        let un = report.iter().find(|p| p.id == "acme/unsigned").unwrap();
        assert_eq!(ok.status, LoadStatus::Loaded);
        assert_eq!(un.status, LoadStatus::Skipped);
    }

    #[test]
    fn load_status_wire_strings_stable() {
        assert_eq!(LoadStatus::Loaded.as_str(), "loaded");
        assert_eq!(LoadStatus::Skipped.as_str(), "skipped");
        assert_eq!(LoadStatus::LoadError.as_str(), "load_error");
    }
}
