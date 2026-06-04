//! Story 19.2 `[rec-2]` — the §5 reproducibility invariant:
//! `reproducibility.class == NonDeterministic` ⟺ `"non_deterministic_pipeline"
//! ∈ tags`. Exercised as a truth table over all 3 classes × {tag, no tag},
//! plus a check that every engine-produced Recommendation satisfies it.

use chrono::{DateTime, Utc};
use opengeo_recommendations::{
    window, ConfidenceBand, Engine, Recommendation, RecommendationKind, Reproducibility,
    ReproducibilityClass, Severity, Traceability, TAG_DETERMINISTIC_LANE, TAG_NON_DETERMINISTIC,
};
use ulid::Ulid;

mod fixture;

fn ts() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-15T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn rec_with(class: ReproducibilityClass, tags: Vec<String>) -> Recommendation {
    Recommendation {
        id: Ulid::from(1u128),
        project_id: Ulid::from(1u128),
        kind: RecommendationKind::DocsNotCitedForPrompt,
        severity: Severity::Medium,
        confidence_band: ConfidenceBand::Medium,
        state: opengeo_recommendations::LifecycleState::New,
        summary: "x".into(),
        payload: serde_json::json!({}),
        traceability: Traceability {
            source_run_ids: vec![Ulid::from(2u128)],
            source_run_ids_truncated: false,
            source_citation_ids: vec![],
            source_citation_ids_truncated: false,
            source_benchmark_queries: vec![],
            window: window(ts(), ts()),
            input_fingerprint: "sha256:abc".into(),
            llm: None,
        },
        reproducibility: Reproducibility { class, note: None },
        tags,
        generated_at: ts(),
        engine_version: "0.5.0".into(),
    }
}

/// Truth table: the helper holds exactly when class==NonDeterministic matches
/// the presence of the binding tag.
#[test]
fn rec_2_invariant_truth_table() {
    let classes = [
        ReproducibilityClass::ByteStable,
        ReproducibilityClass::BestEffort,
        ReproducibilityClass::NonDeterministic,
    ];
    for class in classes {
        for has_tag in [false, true] {
            let tags = if has_tag {
                vec![TAG_NON_DETERMINISTIC.to_string()]
            } else {
                vec![TAG_DETERMINISTIC_LANE.to_string()]
            };
            let rec = rec_with(class, tags);
            let expected = (class == ReproducibilityClass::NonDeterministic) == has_tag;
            assert_eq!(
                rec.reproducibility_invariant_holds(),
                expected,
                "class={class:?} has_tag={has_tag} should hold={expected}"
            );
        }
    }
}

/// Every Recommendation the engine produces satisfies the invariant.
#[test]
fn rec_2_engine_output_satisfies_invariant() {
    let recs = Engine::default().generate(&fixture::full_fixture());
    assert!(!recs.is_empty());
    for r in &recs {
        assert!(
            r.reproducibility_invariant_holds(),
            "engine produced invariant-violating Recommendation for `{}`",
            r.kind.as_str()
        );
    }
}
