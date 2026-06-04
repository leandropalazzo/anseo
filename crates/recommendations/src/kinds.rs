//! The seven deterministic Kind producers (architecture §2.1–2.6, 2.9).
//!
//! Each is a pure function over [`EngineInput`]; triggers follow the §2
//! thresholds. Producers iterate inputs in order, so output is deterministic.

use serde_json::json;

use crate::engine::{
    jaccard_distance, Engine, COMPETITOR_DELTA, COVERAGE_GRAPH_PCT, DOCS_MIN_RUNS, JACCARD_DRIFT,
    RANK_BAD,
};
use crate::input::EngineInput;
use crate::kind::RecommendationKind;
use crate::wire::{BenchmarkQueryRef, ConfidenceBand, Recommendation, Severity};

/// 2.1 — brand docs configured, ≥5 runs, brand docs eTLD+1 absent from
/// citations, but a competitor's docs are present.
pub(crate) fn docs_not_cited_for_prompt(eng: &Engine, input: &EngineInput) -> Vec<Recommendation> {
    let Some(docs) = input.docs_etld1.as_deref() else {
        return vec![];
    };
    let mut out = Vec::new();
    for p in &input.prompts {
        let brand_docs_cited = p.brand_citation_domains.iter().any(|d| d == docs);
        if p.n_runs_14d >= DOCS_MIN_RUNS
            && !brand_docs_cited
            && !p.competitor_docs_present.is_empty()
        {
            let payload = json!({
                "affected_prompt_id": p.prompt_id.to_string(),
                "affected_provider": p.provider_ranks.first().map(|r| r.provider.clone()),
                "competitor_docs_present": p.competitor_docs_present,
            });
            let summary = format!(
                "Your docs ({docs}) are not cited for \"{}\" while competitor docs are",
                p.prompt
            );
            out.push(eng.make_deterministic(
                input,
                RecommendationKind::DocsNotCitedForPrompt,
                Severity::Medium,
                ConfidenceBand::High,
                summary,
                payload,
                &[&p.prompt_id.to_string()],
                p.run_ids.clone(),
                p.citation_ids.clone(),
                vec![],
            ));
        }
    }
    out
}

/// 2.2 — brand mean rank > 3 and a competitor outranks by ≥ 2.
pub(crate) fn competitor_outranks_for_prompt(
    eng: &Engine,
    input: &EngineInput,
) -> Vec<Recommendation> {
    let mut out = Vec::new();
    for p in &input.prompts {
        let Some(brand_rank) = p.brand_mean_rank else {
            continue;
        };
        if brand_rank <= RANK_BAD {
            continue;
        }
        // Most-significant competitor (lowest rank), gated at delta ≥ 2.
        let best = p
            .competitor_ranks
            .iter()
            .filter(|c| brand_rank - c.mean_rank >= COMPETITOR_DELTA)
            .min_by(|a, b| a.mean_rank.total_cmp(&b.mean_rank));
        if let Some(c) = best {
            let delta = brand_rank - c.mean_rank;
            let payload = json!({
                "competitor": c.competitor,
                "brand_rank_mean": brand_rank,
                "competitor_rank_mean": c.mean_rank,
                "delta": delta,
                "n_runs": p.n_runs_14d,
            });
            let summary = format!(
                "{} outranks you for \"{}\" (you {:.1} vs {:.1})",
                c.competitor, p.prompt, brand_rank, c.mean_rank
            );
            out.push(eng.make_deterministic(
                input,
                RecommendationKind::CompetitorOutranksForPrompt,
                Severity::High,
                ConfidenceBand::High,
                summary,
                payload,
                &[&p.prompt_id.to_string(), &c.competitor],
                p.run_ids.clone(),
                vec![],
                vec![],
            ));
        }
    }
    out
}

/// 2.3 — citation-domain Jaccard drift > 0.4 plus the benchmark gate.
pub(crate) fn citation_domain_drift(eng: &Engine, input: &EngineInput) -> Vec<Recommendation> {
    let Some(drift) = input.citation_drift.as_ref() else {
        return vec![];
    };
    let dist = jaccard_distance(&drift.prior_domains, &drift.current_domains);
    if dist <= JACCARD_DRIFT || !drift.current_top3_lower_benchmark_rank {
        return vec![];
    }
    let dropped: Vec<&String> = drift
        .prior_domains
        .iter()
        .filter(|d| !drift.current_domains.contains(d))
        .collect();
    let gained: Vec<&String> = drift
        .current_domains
        .iter()
        .filter(|d| !drift.prior_domains.contains(d))
        .collect();
    let payload = json!({
        "dropped_domains": dropped,
        "gained_domains": gained,
        "jaccard": dist,
    });
    let summary = format!("Citation domain mix shifted (Jaccard distance {dist:.2})");
    vec![eng.make_deterministic(
        input,
        RecommendationKind::CitationDomainDrift,
        Severity::Medium,
        ConfidenceBand::Medium,
        summary,
        payload,
        &["window-drift"],
        vec![],
        drift.citation_ids.clone(),
        vec![],
    )]
}

/// 2.4 — benchmark names a category with no local Prompts but the brand shows
/// up in ≥ 10% of contributors' citation graphs. Requires benchmark opt-in.
pub(crate) fn prompt_coverage_gap(eng: &Engine, input: &EngineInput) -> Vec<Recommendation> {
    if !input.benchmark_opted_in {
        return vec![];
    }
    let mut out = Vec::new();
    for cat in &input.benchmark_categories {
        if !cat.local_has_prompts && cat.brand_in_graph_pct >= COVERAGE_GRAPH_PCT {
            let payload = json!({
                "category": cat.category,
                "suggested_prompts": cat.suggested_prompts,
            });
            let summary = format!(
                "You appear in \"{}\" ({:.0}% of graphs) but track no prompts there",
                cat.category,
                cat.brand_in_graph_pct * 100.0
            );
            out.push(eng.make_deterministic(
                input,
                RecommendationKind::PromptCoverageGap,
                Severity::Low,
                ConfidenceBand::Medium,
                summary,
                payload,
                &[&cat.category],
                vec![],
                vec![],
                vec![BenchmarkQueryRef {
                    name: "recommendation-differences".to_string(),
                    query_hash: cat.query_hash.clone(),
                }],
            ));
        }
    }
    out
}

/// 2.5 — brand cited by ≥1 provider at rank ≤ 3 but by 0 providers in the rest
/// of the enabled cohort.
pub(crate) fn provider_blindspot(eng: &Engine, input: &EngineInput) -> Vec<Recommendation> {
    let mut out = Vec::new();
    for p in &input.prompts {
        let cited_strong = p
            .provider_ranks
            .iter()
            .find(|r| r.cited && r.mean_rank.map(|m| m <= RANK_BAD).unwrap_or(false));
        let Some(cited) = cited_strong else { continue };

        let blind: Vec<String> = input
            .enabled_providers
            .iter()
            .filter(|prov| **prov != cited.provider)
            .filter(|prov| {
                // blind = enabled provider that did NOT cite the brand
                !p.provider_ranks
                    .iter()
                    .any(|r| &r.provider == *prov && r.cited)
            })
            .cloned()
            .collect();

        if !blind.is_empty() {
            let payload = json!({
                "cited_provider": cited.provider,
                "blind_providers": blind,
                "brand_rank_in_cited_provider": cited.mean_rank,
            });
            let summary = format!(
                "Cited by {} for \"{}\" but invisible on {}",
                cited.provider,
                p.prompt,
                blind.join(", ")
            );
            out.push(eng.make_deterministic(
                input,
                RecommendationKind::ProviderBlindspot,
                Severity::Medium,
                ConfidenceBand::High,
                summary,
                payload,
                &[&p.prompt_id.to_string(), &cited.provider],
                p.run_ids.clone(),
                vec![],
                vec![],
            ));
        }
    }
    out
}

/// 2.6 — p50 extraction confidence below the promotion threshold (closes
/// Phase 2 OQ-21). Meta-recommendation.
pub(crate) fn low_extraction_confidence(eng: &Engine, input: &EngineInput) -> Vec<Recommendation> {
    let Some(p50) = input.extraction_p50 else {
        return vec![];
    };
    if p50 >= input.extraction_threshold {
        return vec![];
    }
    let payload = json!({
        "p50_confidence": p50,
        "suggested_plugin": input.extraction_suggested_plugin,
    });
    let summary = format!(
        "Extraction confidence p50 {p50:.2} is below {:.2}; consider an LLM-aided extractor",
        input.extraction_threshold
    );
    vec![eng.make_deterministic(
        input,
        RecommendationKind::LowExtractionConfidence,
        Severity::Low,
        ConfidenceBand::High,
        summary,
        payload,
        &["p50"],
        input.all_run_ids.clone(),
        vec![],
        vec![],
    )]
}

/// 2.9 — brand's category mean rank worse than the benchmark p75 (bottom
/// quartile). Requires benchmark opt-in.
pub(crate) fn benchmark_category_underperformance(
    eng: &Engine,
    input: &EngineInput,
) -> Vec<Recommendation> {
    if !input.benchmark_opted_in {
        return vec![];
    }
    let mut out = Vec::new();
    for cat in &input.benchmark_categories {
        if cat.brand_p_rank > cat.benchmark_p75 {
            let payload = json!({
                "category": cat.category,
                "brand_p_rank": cat.brand_p_rank,
                "benchmark_p75": cat.benchmark_p75,
            });
            let summary = format!(
                "Bottom-quartile in \"{}\" (you {:.1} vs p75 {:.1})",
                cat.category, cat.brand_p_rank, cat.benchmark_p75
            );
            out.push(eng.make_deterministic(
                input,
                RecommendationKind::BenchmarkCategoryUnderperformance,
                Severity::Medium,
                ConfidenceBand::Medium,
                summary,
                payload,
                &[&cat.category],
                input.all_run_ids.clone(),
                vec![],
                vec![BenchmarkQueryRef {
                    name: "category-aggregates".to_string(),
                    query_hash: cat.query_hash.clone(),
                }],
            ));
        }
    }
    out
}
