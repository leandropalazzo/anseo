// Shared test fixture: an `EngineInput` rich enough to trigger every one of
// the seven deterministic Kinds. Used by `determinism.rs`,
// `reproducibility_invariant.rs`, and (via include!) the storage dedup test.

use anseo_recommendations::{
    window, BenchmarkCategory, CitationDriftInput, CompetitorRank, EngineInput, PromptStat,
    ProviderRank, ENGINE_VERSION,
};
use chrono::{DateTime, Utc};
use ulid::Ulid;

pub fn ts() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-15T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

pub fn uid(n: u128) -> Ulid {
    Ulid::from(n)
}

/// A fixture rich enough to trigger every one of the seven deterministic Kinds.
pub fn full_fixture() -> EngineInput {
    EngineInput {
        project_id: uid(1),
        brand: "Acme".into(),
        brand_etld1: "acme.io".into(),
        docs_etld1: Some("acmedocs.io".into()),
        competitors: vec!["Rival".into()],
        enabled_providers: vec!["openai".into(), "anthropic".into(), "gemini".into()],
        benchmark_opted_in: true,
        extraction_p50: Some(0.50),
        extraction_threshold: 0.6,
        extraction_suggested_plugin: "llm-extract".into(),
        prompts: vec![PromptStat {
            prompt_id: uid(100),
            prompt: "best crm".into(),
            run_ids: vec![uid(1000), uid(1001), uid(1002)],
            citation_ids: vec![uid(2000)],
            n_runs_14d: 6,
            brand_mean_rank: Some(5.0),
            provider_ranks: vec![
                ProviderRank {
                    provider: "openai".into(),
                    mean_rank: Some(2.0),
                    cited: true,
                },
                ProviderRank {
                    provider: "anthropic".into(),
                    mean_rank: None,
                    cited: false,
                },
                ProviderRank {
                    provider: "gemini".into(),
                    mean_rank: None,
                    cited: false,
                },
            ],
            competitor_ranks: vec![CompetitorRank {
                competitor: "Rival".into(),
                mean_rank: 2.0,
            }],
            brand_citation_domains: vec!["other.io".into()],
            competitor_docs_present: vec!["rivaldocs.io".into()],
        }],
        citation_drift: Some(CitationDriftInput {
            prior_domains: vec!["a.io".into(), "b.io".into(), "c.io".into()],
            current_domains: vec!["x.io".into(), "y.io".into(), "z.io".into()],
            current_top3_lower_benchmark_rank: true,
            citation_ids: vec![uid(3000), uid(3001)],
        }),
        benchmark_categories: vec![BenchmarkCategory {
            category: "crm".into(),
            brand_p_rank: 10.0,
            benchmark_p75: 5.0,
            brand_in_graph_pct: 0.20,
            local_has_prompts: false,
            suggested_prompts: vec!["best crm for startups".into()],
            query_hash: "sha256:deadbeef".into(),
        }],
        window: window(ts(), ts()),
        generated_at: ts(),
        engine_version: ENGINE_VERSION.into(),
        all_run_ids: vec![uid(1000), uid(1001), uid(1002)],
    }
}
