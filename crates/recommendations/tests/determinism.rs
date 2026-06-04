//! Story 19.1 contracts: `[rec-1]` byte-stable determinism + `[rec-6]`
//! traceability obligation, exercised across all seven deterministic Kinds.

use std::collections::BTreeSet;

use opengeo_recommendations::{Engine, RecommendationKind, ReproducibilityClass, ENGINE_VERSION};

mod fixture;
use fixture::full_fixture;

/// `[rec-1]` — two runs against the same fixture produce a byte-identical
/// wire payload (the whole list, ids included).
#[test]
fn rec_1_output_is_byte_stable_across_runs() {
    let eng = Engine::default();
    let input = full_fixture();

    let a = serde_json::to_vec(&eng.generate(&input)).unwrap();
    let b = serde_json::to_vec(&eng.generate(&input)).unwrap();

    assert_eq!(
        a, b,
        "deterministic-lane output must be byte-identical across runs"
    );
    assert!(!a.is_empty(), "fixture should produce recommendations");
}

/// All seven deterministic Kinds fire on the full fixture (coverage for the
/// per-Kind obligation checks below).
#[test]
fn all_seven_deterministic_kinds_fire() {
    let recs = Engine::default().generate(&full_fixture());
    let produced: BTreeSet<&'static str> = recs.iter().map(|r| r.kind.as_str()).collect();
    for k in RecommendationKind::deterministic() {
        assert!(
            produced.contains(k.as_str()),
            "Kind `{}` did not fire on the full fixture",
            k.as_str()
        );
    }
}

/// `[rec-6]` — every Recommendation carries real traceability, and the §5
/// reproducibility invariant holds. Empty traceability is a hard bug.
#[test]
fn rec_6_every_recommendation_has_traceability_and_holds_invariant() {
    let recs = Engine::default().generate(&full_fixture());
    assert!(!recs.is_empty());
    for r in &recs {
        assert!(
            r.traceability.is_non_empty(),
            "Kind `{}` produced empty traceability",
            r.kind.as_str()
        );
        assert!(!r.traceability.input_fingerprint.is_empty());
        assert!(
            r.reproducibility_invariant_holds(),
            "reproducibility/tag invariant violated for `{}`",
            r.kind.as_str()
        );
        // Deterministic lane: byte-stable, never tagged non-deterministic.
        assert_eq!(r.reproducibility.class, ReproducibilityClass::ByteStable);
        assert!(!r
            .tags
            .iter()
            .any(|t| t == opengeo_recommendations::TAG_NON_DETERMINISTIC));
        assert_eq!(r.engine_version, ENGINE_VERSION);
    }
}

/// Content-derived ids are stable run-to-run (the dedup identity Story 19.2
/// builds on).
#[test]
fn ids_are_content_derived_and_stable() {
    let eng = Engine::default();
    let input = full_fixture();
    let first: Vec<_> = eng.generate(&input).iter().map(|r| r.id).collect();
    let second: Vec<_> = eng.generate(&input).iter().map(|r| r.id).collect();
    assert_eq!(first, second, "ids must be deterministic (content-derived)");
}

/// A non-triggering fixture produces nothing (no false positives).
#[test]
fn empty_signals_produce_no_recommendations() {
    let mut input = full_fixture();
    // Remove every trigger.
    input.docs_etld1 = None;
    input.benchmark_opted_in = false;
    input.extraction_p50 = Some(0.9);
    input.citation_drift = None;
    input.prompts[0].brand_mean_rank = Some(1.0); // strong rank, no competitor gap
    input.prompts[0].competitor_docs_present.clear();
    input.prompts[0]
        .provider_ranks
        .iter_mut()
        .for_each(|r| r.cited = true);
    let recs = Engine::default().generate(&input);
    assert!(
        recs.is_empty(),
        "no triggers should yield no recommendations, got {recs:?}"
    );
}
