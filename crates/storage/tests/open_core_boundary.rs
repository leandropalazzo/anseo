#[test]
fn storage_crate_has_no_premium_hallucination_dependency() {
    let manifest = std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")))
        .expect("read storage Cargo.toml");

    assert!(
        !manifest.contains("opengeo-hallucination"),
        "OSS storage must not depend on the premium hallucination evaluator"
    );
}
