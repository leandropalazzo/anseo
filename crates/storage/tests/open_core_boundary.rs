#[test]
fn storage_crate_has_no_premium_hallucination_dependency() {
    let manifest = std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")))
        .expect("read storage Cargo.toml");

    assert!(
        !manifest.contains("opengeo-hallucination"),
        "OSS storage must not depend on the premium hallucination evaluator"
    );
}

#[test]
fn oss_vs_commercial_matrix_lists_hallucination_entitlement() {
    let matrix = std::fs::read_to_string(format!(
        "{}/../../_bmad-output/planning-artifacts/prds/prd-OpenGEO-full-2026-05-22/prd.md",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read OSS-vs-commercial matrix");

    assert!(matrix.contains("Claim + ground-truth storage"));
    assert!(matrix.contains("phase4.hallucination_monitoring"));
}
