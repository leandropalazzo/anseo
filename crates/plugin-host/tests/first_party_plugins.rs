//! Story 41.5 — first-party reference plugins: validate + load roundtrip.
//!
//! Proves each bundled manifest under the repo `plugins/` tree:
//!   1. parses against the on-disk `PluginManifest` schema,
//!   2. passes `PluginManifest::validate()` (DNS-safe name, semver, capability
//!      present, relative entry_point),
//!   3. declares the kind the story requires, and
//!   4. reaches `LoadStatus::Loaded` through the runtime loader `scan_and_load`,
//!      exercising the load *decision* (manifest read + parse, signature-status
//!      gate, platform sandbox gate) against a realistically-staged bundle.
//!
//! Scope note — what the load gate does and does NOT check today:
//!   `scan_and_load` decides load/skip/error from (a) the on-disk manifest, (b)
//!   the `signature_status` recorded in `installed.toml`, and (c) the platform
//!   sandbox capability. It does **not** itself verify that `entrypoint.wasm`
//!   exists or recompute/verify the Ed25519 signature over the bundle bytes at
//!   load time — that load-time artifact-presence + signature verification is a
//!   host hardening item tracked separately (the subprocess/loader hardening
//!   follow-up; see plugins/README.md). To keep this test high-fidelity (and
//!   not green for a bundle missing its entrypoint), we still stage a *real*
//!   built `entrypoint.wasm` next to each manifest, mirroring exactly what the
//!   installer materializes under `<home>/plugins/<id>/<version>/`.
//!
//! Hermetic: the manifest is read from the repo and staged — together with a
//! freshly built artifact — into a tempdir that mirrors the install layout. No
//! network, no real download. The built artifact lives only in the tempdir and
//! is never committed.

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

    // Stage each first-party plugin into the install layout exactly as the
    // installer materializes it: the manifest PLUS a freshly built
    // `entrypoint.wasm`, recorded as `signed` (the 41.4 CI pipeline signs them).
    // Building a real artifact keeps this test honest — a bundle that is missing
    // its entrypoint would not match what `ogeo plugin install` writes, even
    // though the current `scan_and_load` decision does not yet read the artifact
    // (load-time artifact-presence + signature verification is the deferred host
    // hardening item; see the module doc and plugins/README.md). Use a
    // Linux-pinned policy so the analytics sandbox gate is host-independent.
    let mut installed = String::new();
    for (dir, id, kind) in first_party() {
        let src = repo_root().join("plugins").join(dir).join("manifest.yaml");
        let dst_dir = home.join("plugins").join(id).join("0.1.0");
        fs::create_dir_all(&dst_dir).expect("create install dir");
        fs::copy(&src, dst_dir.join("manifest.yaml")).expect("stage manifest");

        // Build the plugin's native artifact and stage it under the conventional
        // `entrypoint.wasm` filename (the install layout uses that name
        // regardless of artifact kind — see plugins/build.sh). The artifact is
        // written into the tempdir only and is never committed to git.
        let artifact = build_plugin_artifact(dir, kind);
        fs::copy(&artifact, dst_dir.join("entrypoint.wasm")).expect("stage entrypoint.wasm");
        assert!(
            dst_dir.join("entrypoint.wasm").is_file(),
            "{id}: staged bundle must contain a real entrypoint.wasm"
        );

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

    assert_eq!(report.len(), 2, "both first-party plugins reported");
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
    assert!(
        status.success(),
        "{dir}: cargo build --release must succeed"
    );

    // Cargo appends the platform executable suffix (e.g. `.exe` on Windows).
    let bin = plugin_dir
        .join("target")
        .join("release")
        .join(format!("{bin_name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        bin.exists(),
        "{dir}: built binary must exist at {}",
        bin.display()
    );
    bin
}

/// Build a real artifact for a first-party plugin and return its path, so the
/// load-roundtrip test can stage a genuine `entrypoint.wasm` matching what the
/// installer materializes. The artifact kind tracks the plugin kind:
///
///   * Analytics → native subprocess binary (the exact artifact the sandbox
///     spawns); reuses [`build_subprocess_plugin`] so we build it the same way
///     the execution test does.
///   * Provider → native cdylib (`.dylib`/`.so`/`.dll` per host). We build the
///     *native* cdylib rather than the `wasm32-wasip1` target so the test never
///     depends on a rustup wasm target being installed in CI; it is still a
///     real, freshly compiled artifact (not an empty placeholder). The on-disk
///     install convention names it `entrypoint.wasm` regardless of true format
///     — see plugins/build.sh.
///
/// The path returned points into the plugin's own `target/` dir; the caller
/// copies it into a tempdir and never commits it.
fn build_plugin_artifact(dir: &str, kind: PluginType) -> PathBuf {
    match kind {
        PluginType::Analytics => build_subprocess_plugin(dir, dir),
        _ => build_native_cdylib(dir),
    }
}

/// Build the plugin crate's native cdylib in its own workspace and return the
/// produced shared-library path (resolved by extension, host-portable).
fn build_native_cdylib(dir: &str) -> PathBuf {
    let plugin_dir = repo_root().join("plugins").join(dir);
    let status = Command::new(env!("CARGO"))
        .arg("build")
        .arg("--release")
        .current_dir(&plugin_dir)
        .status()
        .unwrap_or_else(|e| panic!("{dir}: failed to spawn cargo build: {e}"));
    assert!(
        status.success(),
        "{dir}: cargo build --release must succeed"
    );

    // cdylib output name follows the host's platform conventions:
    // DLL_PREFIX is "lib" on unix and "" on Windows; DLL_SUFFIX is
    // ".so"/".dylib"/".dll". The crate name has `-` normalized to `_`.
    let lib_name = format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        dir.replace('-', "_"),
        std::env::consts::DLL_SUFFIX,
    );
    let release = plugin_dir.join("target").join("release");
    let candidate = release.join(&lib_name);
    if candidate.is_file() {
        return candidate;
    }
    panic!(
        "{dir}: expected a cdylib ({lib_name}) under {}",
        release.display()
    );
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
    let request = r#"{"window":"30d","metric":"citation_share","points":[0.10,0.12,0.15,0.19]}"#;
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
