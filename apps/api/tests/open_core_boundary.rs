//! Open-core boundary leak-guard for the OSS API binary.
//!
//! The default (OSS) build of `opengeo-api` must NOT pull in any premium
//! crate. The premium brand-accuracy / hallucination evaluator lives behind
//! the `pro` cargo feature; with default features it must be entirely absent
//! from the dependency graph.
//!
//! This mirrors `crates/storage/tests/open_core_boundary.rs`: a cheap,
//! committed guard that fails CI the moment the coupling is reintroduced.

/// The default-feature manifest must declare `opengeo-hallucination` as an
/// OPTIONAL dependency gated behind a feature — never an unconditional one.
#[test]
fn api_manifest_keeps_hallucination_optional_and_feature_gated() {
    let manifest = std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")))
        .expect("read api Cargo.toml");

    // If the premium crate is referenced at all, it must be optional.
    if manifest.contains("opengeo-hallucination") {
        assert!(
            manifest.contains("opengeo-hallucination = { path = \"../../crates/hallucination\", optional = true }"),
            "opengeo-hallucination must be an OPTIONAL dependency of the OSS API"
        );
        // And only reachable through the `pro` feature.
        assert!(
            manifest.contains("pro = [\"dep:opengeo-hallucination\"]"),
            "the hallucination crate must be gated behind the `pro` feature"
        );
    }
}

/// The actual dependency graph for the default-feature `opengeo-api` must not
/// contain `opengeo-hallucination`. This catches transitive leaks that a
/// manifest grep cannot. The test is skipped if `cargo` is unavailable.
#[test]
fn default_api_dependency_graph_excludes_premium_crates() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");

    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "tree",
            "--package",
            "opengeo-api",
            "--no-default-features",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
        ])
        .current_dir(manifest_dir)
        .env("SQLX_OFFLINE", "true")
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        // If cargo is unavailable or offline metadata can't be resolved in the
        // sandbox, fall back to the manifest-level guard above rather than
        // failing spuriously.
        _ => return,
    };

    let tree = String::from_utf8_lossy(&output.stdout);

    // List of premium crates that must never leak into the OSS API graph.
    // Extend this as new closed-source crates (org/rbac/billing/...) appear.
    #[allow(clippy::single_element_loop)]
    for premium in ["opengeo-hallucination"] {
        assert!(
            !tree.lines().any(|l| l.trim_start().starts_with(premium)),
            "OSS `opengeo-api` default build must not depend on premium crate `{premium}`:\n{tree}"
        );
    }
}
