//! Story 19.3 — LLM-aided / hybrid Kind producers (architecture §2.7, §2.8,
//! §2.10). Each calls the synchronous [`EnrichmentProvider`] boundary and
//! builds its Recommendation via [`Engine::make_llm`], which derives the
//! reproducibility class + binding tag from the determinism allow-list.
//!
//! Privacy (§9.4): the rendered prompt is restricted to the Project's own
//! brand/docs config — never raw user Prompt text.

use serde_json::json;

use crate::engine::Engine;
use crate::input::EngineInput;
use crate::kind::RecommendationKind;
use crate::llm::{EnrichmentProvider, EnrichmentRequest, LlmConfig};
use crate::wire::{ConfidenceBand, LlmTrace, Recommendation, Severity};

/// Build the LlmTrace for one enrichment call, honoring the per-call token cap.
fn call(
    provider: &dyn EnrichmentProvider,
    cfg: &LlmConfig,
    full_prompt: String,
) -> Option<LlmTrace> {
    // Allow-listed providers get temperature=0 + a fixed seed; others run at
    // the provider default (the resulting Recommendation is tagged
    // non_deterministic_pipeline regardless).
    let req = EnrichmentRequest {
        full_prompt: full_prompt.clone(),
        temperature: 0.0,
        seed: Some(7),
        max_completion_tokens: cfg.max_completion_tokens,
    };
    let outcome = provider.enrich(&req).ok()?;
    // Per-call cost cap (§4.4): drop the enrichment if the provider blew the
    // completion budget rather than persisting an over-budget trace.
    if outcome.tokens_out > cfg.max_completion_tokens || outcome.tokens_in > cfg.max_prompt_tokens {
        return None;
    }
    let (tpl_id, tpl_ver) = provider.template();
    Some(LlmTrace {
        provider: provider.provider_id(),
        model_id: provider.model_id().to_string(),
        model_content_hash: outcome.model_content_hash,
        prompt_template_id: tpl_id.to_string(),
        prompt_template_version: tpl_ver.to_string(),
        full_prompt,
        full_response: outcome.full_response,
        temperature: Some(0.0),
        seed: Some(7),
        latency_ms: outcome.latency_ms,
        tokens_in: outcome.tokens_in,
        tokens_out: outcome.tokens_out,
    })
}

/// Produce the LLM-aided + hybrid Recommendations for one run. `det` is the
/// already-generated deterministic set, used to anchor the enrichment Kinds
/// (2.7 enriches parent 2.1 / 2.2 Recommendations per §2.7).
pub(crate) fn enrich(
    eng: &Engine,
    input: &EngineInput,
    cfg: &LlmConfig,
    provider: &dyn EnrichmentProvider,
    det: &[Recommendation],
) -> Vec<Recommendation> {
    let mut out = Vec::new();
    out.extend(structural_content_suggestion(
        eng, input, cfg, provider, det,
    ));
    out.extend(citation_quality_uplift(eng, input, cfg, provider));
    out.extend(volatility_anomaly_explained(eng, input, cfg, provider));
    out
}

/// 2.7 structural_content_suggestion (LLM-aided). Never fires independently —
/// enriches a parent `docs_not_cited_for_prompt` / `competitor_outranks_for_prompt`.
fn structural_content_suggestion(
    eng: &Engine,
    input: &EngineInput,
    cfg: &LlmConfig,
    provider: &dyn EnrichmentProvider,
    det: &[Recommendation],
) -> Vec<Recommendation> {
    let mut out = Vec::new();
    for parent in det.iter().filter(|r| {
        matches!(
            r.kind,
            RecommendationKind::DocsNotCitedForPrompt
                | RecommendationKind::CompetitorOutranksForPrompt
        )
    }) {
        let prompt = format!(
            "For brand {} (docs {:?}), suggest a concrete structural content change \
             addressing: {}",
            input.brand, input.docs_etld1, parent.summary
        );
        let Some(llm) = call(provider, cfg, prompt) else {
            continue;
        };
        let payload = json!({
            "parent_recommendation_id": parent.id.to_string(),
            "parent_kind": parent.kind.as_str(),
            "suggestion": llm.full_response,
        });
        out.push(eng.make_llm(
            input,
            RecommendationKind::StructuralContentSuggestion,
            Severity::Medium,
            ConfidenceBand::Medium,
            format!("Structural suggestion for \"{}\"", parent.summary),
            payload,
            &[parent.id.to_string().as_str()],
            parent.traceability.source_run_ids.clone(),
            parent.traceability.source_citation_ids.clone(),
            llm,
        ));
    }
    out
}

/// 2.8 citation_quality_uplift (LLM-aided). Fires per tracked Prompt that has
/// citations to reason over.
fn citation_quality_uplift(
    eng: &Engine,
    input: &EngineInput,
    cfg: &LlmConfig,
    provider: &dyn EnrichmentProvider,
) -> Vec<Recommendation> {
    let mut out = Vec::new();
    for p in input.prompts.iter().filter(|p| !p.citation_ids.is_empty()) {
        let prompt = format!(
            "For brand {} on topic \"{}\", suggest how to improve citation quality.",
            input.brand, p.prompt
        );
        let Some(llm) = call(provider, cfg, prompt) else {
            continue;
        };
        let payload = json!({
            "affected_prompt_id": p.prompt_id.to_string(),
            "uplift": llm.full_response,
        });
        out.push(eng.make_llm(
            input,
            RecommendationKind::CitationQualityUplift,
            Severity::Low,
            ConfidenceBand::Medium,
            format!("Improve citation quality for \"{}\"", p.prompt),
            payload,
            &[p.prompt_id.to_string().as_str()],
            p.run_ids.clone(),
            p.citation_ids.clone(),
            llm,
        ));
    }
    out
}

/// 2.10 volatility_anomaly_explained (hybrid). Deterministic core anchored on
/// citation drift; LLM enrichment fields populated, which is what sets the
/// `non_deterministic_pipeline` tag (per §2.10).
fn volatility_anomaly_explained(
    eng: &Engine,
    input: &EngineInput,
    cfg: &LlmConfig,
    provider: &dyn EnrichmentProvider,
) -> Vec<Recommendation> {
    let Some(drift) = input.citation_drift.as_ref() else {
        return vec![];
    };
    let prompt = format!(
        "Brand {} saw a citation-domain shift. Explain the likely cause.",
        input.brand
    );
    let Some(llm) = call(provider, cfg, prompt) else {
        return vec![];
    };
    let payload = json!({
        "prior_domains": drift.prior_domains,
        "current_domains": drift.current_domains,
        "explanation": llm.full_response,
    });
    vec![eng.make_llm(
        input,
        RecommendationKind::VolatilityAnomalyExplained,
        Severity::Medium,
        ConfidenceBand::Medium,
        "Volatility anomaly explained".to_string(),
        payload,
        &["window-volatility"],
        vec![],
        drift.citation_ids.clone(),
        llm,
    )]
}
