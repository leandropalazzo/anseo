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
use std::path::{Path, PathBuf};
use std::process::Command;

use anseo_plugin_host::loader::{scan_and_load, LoadPolicy, LoadStatus};
use anseo_plugin_host::subprocess::{run, AnalyticsSandbox, Platform, RunOutcome};
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
            "anseo-trend-analytics",
            "anseo/anseo-trend-analytics",
            PluginType::Analytics,
        ),
        (
            "anseo-ndjson-export",
            "anseo/anseo-ndjson-export",
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

// ---------------------------------------------------------------------------
// Real execution tests — prove the subprocess plugins are NOT stubs.
//
// These compile each native subprocess plugin (its own standalone workspace),
// then invoke it through the *exact* host execution primitive
// (`subprocess::run`, the analytics sandbox) with the request passed as args,
// and assert a well-formed, non-empty JSON result on stdout. A stub that only
// logged to stderr would fail: stderr is discarded by `run`, so the only way to
// pass is to do real work and write the result to stdout.
// ---------------------------------------------------------------------------

/// Build the named plugin's release binary in its own workspace and return the
/// path to the produced executable. Skips the build's interaction with the host
/// workspace because each plugin Cargo.toml carries an empty `[workspace]`.
fn build_subprocess_plugin(dir: &str, bin_name: &str) -> PathBuf {
    let plugin_dir = repo_root().join("plugins").join(dir);
    let status = Command::new(env!("CARGO"))
        .arg("build")
        .arg("--release")
        .arg("--bin")
        .arg(bin_name)
        .current_dir(&plugin_dir)
        .status()
        .unwrap_or_else(|e| panic!("{dir}: failed to spawn cargo build: {e}"));
    assert!(status.success(), "{dir}: cargo build --release must succeed");

    let bin = plugin_dir
        .join("target")
        .join("release")
        .join(bin_name);
    assert!(
        bin.exists(),
        "{dir}: built binary must exist at {}",
        bin.display()
    );
    bin
}

/// Run a built plugin binary through the real host sandbox primitive with the
/// request as argv[1], returning the captured stdout as a String.
fn run_through_sandbox(bin: &Path, request: &str) -> String {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let sandbox = AnalyticsSandbox::defaults(scratch.path().to_path_buf());
    let outcome = run(
        Platform::current(),
        &sandbox,
        &bin.to_string_lossy(),
        &[request],
    )
    .expect("sandboxed run spawns");

    match outcome {
        RunOutcome::Exited { code, stdout } => {
            assert_eq!(code, 0, "plugin must exit 0; stdout was empty? proves work");
            assert!(!stdout.is_empty(), "plugin must write a result to stdout");
            String::from_utf8(stdout).expect("stdout is valid UTF-8")
        }
        RunOutcome::Timeout => panic!("plugin timed out under the sandbox"),
    }
}

#[test]
fn trend_analytics_plugin_executes_and_emits_trend_result() {
    if !Platform::current().supports_analytics_subprocess() {
        eprintln!("skipping: analytics subprocess unsupported on this platform");
        return;
    }
    let bin = build_subprocess_plugin("anseo-trend-analytics", "anseo-trend-analytics");
    let request =
        r#"{"window":"30d","metric":"citation_share","points":[0.10,0.12,0.15,0.19]}"#;
    let stdout = run_through_sandbox(&bin, request);

    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout is well-formed JSON");
    // A real computation, namespaced trend kind, rising series → rising.
    assert_eq!(v["trend_kind"], "plugin:anseo-trend-analytics:rollup");
    assert_eq!(v["metric"], "citation_share");
    assert_eq!(v["window"], "30d");
    assert_eq!(v["count"], 4);
    assert_eq!(v["direction"], "rising");
    assert!(
        v["slope"].as_f64().expect("slope is numeric") > 0.0,
        "rising series must have positive slope"
    );
}

#[test]
fn ndjson_export_plugin_executes_and_emits_ndjson() {
    if !Platform::current().supports_analytics_subprocess() {
        eprintln!("skipping: output subprocess unsupported on this platform");
        return;
    }
    let bin = build_subprocess_plugin("anseo-ndjson-export", "anseo-ndjson-export");
    let request =
        r#"{"run_id":"r-123","rows":[{"prompt":"p1","score":0.8},{"prompt":"p2","score":0.4}]}"#;
    let stdout = run_through_sandbox(&bin, request);

    let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "one NDJSON record per input row");

    let l0: serde_json::Value =
        serde_json::from_str(lines[0]).expect("line 0 is well-formed JSON");
    assert_eq!(l0["run_id"], "r-123");
    assert_eq!(l0["prompt"], "p1");
    assert_eq!(l0["score"], 0.8);

    let l1: serde_json::Value =
        serde_json::from_str(lines[1]).expect("line 1 is well-formed JSON");
    assert_eq!(l1["prompt"], "p2");
}
