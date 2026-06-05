//! Story 19.3 — LLM-aided / hybrid lane: fallback warning `[rec-5]`, FR-60
//! local-OSS intersection `[rec-8]`, determinism allow-list reflection, cost
//! caps, and per-Kind firing with a stubbed Provider.

use anseo_recommendations::{
    Engine, LlmConfig, RecommendationKind, ReproducibilityClass, StubProvider,
    TAG_NON_DETERMINISTIC,
};

mod fixture;
use fixture::full_fixture;

fn enrich_cfg() -> LlmConfig {
    LlmConfig {
        enrich: true,
        ..LlmConfig::default()
    }
}

fn llm_kinds(recs: &[anseo_recommendations::Recommendation]) -> Vec<RecommendationKind> {
    recs.iter()
        .map(|r| r.kind)
        .filter(|k| {
            matches!(
                k,
                RecommendationKind::StructuralContentSuggestion
                    | RecommendationKind::CitationQualityUplift
                    | RecommendationKind::VolatilityAnomalyExplained
            )
        })
        .collect()
}

/// `[rec-5]` — `llm_enrich: true` with no Provider returns the deterministic
/// set unchanged plus a single `llm_enrichment_skipped` warning.
#[test]
fn rec_5_no_provider_emits_single_skip_warning() {
    let eng = Engine::default();
    let det_only = eng.generate(&full_fixture());

    let out = eng.generate_with_llm(&full_fixture(), &enrich_cfg(), None);

    assert_eq!(out.warnings.len(), 1);
    assert_eq!(out.warnings[0].kind, "llm_enrichment_skipped");
    assert_eq!(out.warnings[0].reason, "no_provider_configured");
    assert_eq!(
        out.recommendations.len(),
        det_only.len(),
        "deterministic set returned unchanged"
    );
    assert!(
        llm_kinds(&out.recommendations).is_empty(),
        "no LLM-aided Kinds without a Provider"
    );
}

/// Default config (enrich=false) never invokes the lane, even with a Provider.
#[test]
fn enrich_disabled_returns_deterministic_only() {
    let eng = Engine::default();
    let provider = StubProvider::deterministic_openai();
    let out = eng.generate_with_llm(&full_fixture(), &LlmConfig::default(), Some(&provider));
    assert!(out.warnings.is_empty());
    assert!(llm_kinds(&out.recommendations).is_empty());
}

/// All three LLM-aided / hybrid Kinds fire on the full fixture with a Provider.
#[test]
fn per_kind_llm_fires() {
    let eng = Engine::default();
    let provider = StubProvider::deterministic_openai();
    let out = eng.generate_with_llm(&full_fixture(), &enrich_cfg(), Some(&provider));
    let kinds = llm_kinds(&out.recommendations);
    assert!(kinds.contains(&RecommendationKind::StructuralContentSuggestion));
    assert!(kinds.contains(&RecommendationKind::CitationQualityUplift));
    assert!(kinds.contains(&RecommendationKind::VolatilityAnomalyExplained));
    // Every Recommendation (det + LLM) holds the §5 invariant.
    for r in &out.recommendations {
        assert!(
            r.reproducibility_invariant_holds(),
            "invariant broke for {}",
            r.kind.as_str()
        );
    }
}

/// Allow-list reflection: an allow-listed provider yields best-effort recs with
/// NO non-deterministic tag; a non-allow-listed provider tags every LLM rec.
#[test]
fn allow_list_reflection() {
    let eng = Engine::default();

    let det = StubProvider::deterministic_openai();
    let det_out = eng.generate_with_llm(&full_fixture(), &enrich_cfg(), Some(&det));
    for r in det_out
        .recommendations
        .iter()
        .filter(|r| r.traceability.llm.is_some())
    {
        assert_eq!(r.reproducibility.class, ReproducibilityClass::BestEffort);
        assert!(!r.tags.iter().any(|t| t == TAG_NON_DETERMINISTIC));
        assert!(r.reproducibility_invariant_holds());
    }

    let nd = StubProvider::non_deterministic_anthropic();
    let nd_out = eng.generate_with_llm(&full_fixture(), &enrich_cfg(), Some(&nd));
    let llm_recs: Vec<_> = nd_out
        .recommendations
        .iter()
        .filter(|r| r.traceability.llm.is_some())
        .collect();
    assert!(!llm_recs.is_empty());
    for r in llm_recs {
        assert_eq!(
            r.reproducibility.class,
            ReproducibilityClass::NonDeterministic
        );
        assert!(r.tags.iter().any(|t| t == TAG_NON_DETERMINISTIC));
        assert!(r.reproducibility_invariant_holds());
    }
}

/// `[rec-8]` — local-OSS endpoint sets `llm.provider == "local-oss:..."`, and
/// reproducibility class reflects model-content-hash presence.
#[test]
fn rec_8_local_oss_intersection() {
    let eng = Engine::default();

    let with_hash = StubProvider::local_oss(true);
    let out = eng.generate_with_llm(&full_fixture(), &enrich_cfg(), Some(&with_hash));
    let recs: Vec<_> = out
        .recommendations
        .iter()
        .filter(|r| r.traceability.llm.is_some())
        .collect();
    assert!(!recs.is_empty());
    for r in &recs {
        let llm = r.traceability.llm.as_ref().unwrap();
        assert_eq!(llm.provider, "local-oss:llm.lan");
        assert!(llm.model_content_hash.is_some());
        assert_eq!(r.reproducibility.class, ReproducibilityClass::BestEffort);
        assert!(!r.tags.iter().any(|t| t == TAG_NON_DETERMINISTIC));
    }

    let no_hash = StubProvider::local_oss(false);
    let out = eng.generate_with_llm(&full_fixture(), &enrich_cfg(), Some(&no_hash));
    for r in out
        .recommendations
        .iter()
        .filter(|r| r.traceability.llm.is_some())
    {
        let llm = r.traceability.llm.as_ref().unwrap();
        assert_eq!(llm.provider, "local-oss:llm.lan");
        assert!(llm.model_content_hash.is_none());
        assert_eq!(
            r.reproducibility.class,
            ReproducibilityClass::NonDeterministic
        );
        assert!(r.tags.iter().any(|t| t == TAG_NON_DETERMINISTIC));
    }
}

/// Cost cap: ≤ max_recs_per_run LLM-aided recs / run (§4.4).
#[test]
fn cost_cap_limits_llm_recs_per_run() {
    let eng = Engine::default();
    let provider = StubProvider::deterministic_openai();
    let cfg = LlmConfig {
        enrich: true,
        max_recs_per_run: 1,
        ..LlmConfig::default()
    };
    let out = eng.generate_with_llm(&full_fixture(), &cfg, Some(&provider));
    assert_eq!(
        llm_kinds(&out.recommendations).len(),
        1,
        "LLM-aided recs capped at 1"
    );
}

/// `[rec-1]` extends: with a deterministic stub Provider the full run is
/// byte-stable across two invocations (hybrid core included).
#[test]
fn rec_1_hybrid_run_is_byte_stable() {
    let eng = Engine::default();
    let provider = StubProvider::deterministic_openai();
    let a = serde_json::to_vec(
        &eng.generate_with_llm(&full_fixture(), &enrich_cfg(), Some(&provider))
            .recommendations,
    )
    .unwrap();
    let b = serde_json::to_vec(
        &eng.generate_with_llm(&full_fixture(), &enrich_cfg(), Some(&provider))
            .recommendations,
    )
    .unwrap();
    assert_eq!(a, b);
}
