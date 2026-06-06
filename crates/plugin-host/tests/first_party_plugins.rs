//! Story 41.5 — first-party reference plugins: validate + load roundtrip.
//!
//! Proves each bundled manifest under the repo `plugins/` tree:
//!   1. parses against the on-disk `PluginManifest` schema,
//!   2. passes `PluginManifest::validate()` (DNS-safe name, semver, capability
//!      present, relative entry_point),
//!   3. declares the kind the story requires, and
//!   4. loads cleanly through the runtime loader `scan_and_load` — i.e. it
//!      reaches `LoadStatus::Loaded` through the exact gates `anseo serve`
//!      applies (signed + sandbox-supported platform).
//!
//! Hermetic: the manifest is read from the repo and staged into a tempdir that
//! mirrors the `<home>/plugins/<id>/<version>/` install layout. No network, no
//! real download.

use std::fs;
use std::path::PathBuf;

use anseo_plugin_host::loader::{scan_and_load, LoadPolicy, LoadStatus};
use anseo_plugin_host::subprocess::Platform;
use anseo_plugin_manifest::{PluginManifest, PluginType};

/// Repo root, derived from this crate's manifest dir (`crates/plugin-host`).
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/plugin-host has a grandparent (repo root)")
        .to_path_buf()
}

/// (directory under `plugins/`, expected registry id, expected kind).
fn first_party() -> Vec<(&'static str, &'static str, PluginType)> {
    vec![
        (
            "anseo-warehouse",
            "anseo/anseo-warehouse",
            PluginType::Analytics,
        ),
        (
            "anseo-connect-bigquery",
            "anseo/anseo-connect-bigquery",
            PluginType::OutputFormat,
        ),
        (
            "anseo-example-provider",
            "anseo/anseo-example-provider",
            PluginType::Provider,
        ),
    ]
}

#[test]
fn first_party_manifests_parse_and_validate() {
    for (dir, _id, expected_kind) in first_party() {
        let path = repo_root().join("plugins").join(dir).join("manifest.yaml");
        let manifest = PluginManifest::load_from_path(&path)
            .unwrap_or_else(|e| panic!("{dir}: manifest must parse: {e}"));

        manifest
            .validate()
            .unwrap_or_else(|errs| panic!("{dir}: manifest must validate: {errs:?}"));

        assert_eq!(
            manifest.plugin_type, expected_kind,
            "{dir}: declares the expected kind"
        );
        // The bare manifest name must be DNS-safe and match the directory.
        assert_eq!(manifest.name, dir, "{dir}: manifest name matches directory");
        assert!(
            !manifest.name.contains('/') && !manifest.name.contains(':'),
            "{dir}: name is the bare DNS-safe name (no namespace separator)"
        );
        assert!(
            !manifest.capabilities.is_empty(),
            "{dir}: declares at least one capability"
        );
    }
}

#[test]
fn first_party_plugins_load_through_loader() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();

    // Stage each first-party manifest into the install layout, recorded as
    // `signed` (the 41.4 CI pipeline signs them). Use a Linux-pinned policy so
    // the analytics sandbox gate is host-independent in CI.
    let mut installed = String::new();
    for (dir, id, _kind) in first_party() {
        let src = repo_root().join("plugins").join(dir).join("manifest.yaml");
        let dst_dir = home.join("plugins").join(id).join("0.1.0");
        fs::create_dir_all(&dst_dir).expect("create install dir");
        fs::copy(&src, dst_dir.join("manifest.yaml")).expect("stage manifest");
        installed.push_str(&format!(
            "[[plugin]]\nid = \"{id}\"\nversion = \"0.1.0\"\nsignature_status = \"signed\"\nnamespace = \"anseo\"\n",
        ));
    }
    fs::write(home.join("installed.toml"), installed).expect("write installed.toml");

    let policy = LoadPolicy {
        allow_unsigned: false,
        platform: Platform::Linux,
    };
    let report = scan_and_load(home, &policy);

    assert_eq!(report.len(), 3, "all three first-party plugins reported");
    for (_dir, id, kind) in first_party() {
        let row = report
            .iter()
            .find(|p| p.id == id)
            .unwrap_or_else(|| panic!("{id}: present in load report"));
        assert_eq!(
            row.status,
            LoadStatus::Loaded,
            "{id}: loads cleanly (reason: {})",
            row.reason
        );
        assert_eq!(row.kind, kind.to_string(), "{id}: reports its kind");
        assert!(row.reason.is_empty(), "{id}: clean load has no reason");
    }
}
