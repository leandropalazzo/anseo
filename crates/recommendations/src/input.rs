//! Pre-aggregated inputs the deterministic engine consumes.
//!
//! The engine never queries a DB itself (AD-Phase3-RecommendationsInProcess):
//! a consumer (Story 19.6) populates this bag from Postgres/ClickHouse/benchmark
//! aggregates. Keeping the engine pure over plain data is what makes the
//! `[rec-1]` byte-stable contract testable offline.

use chrono::{DateTime, Utc};
use ulid::Ulid;

use crate::wire::TimeWindow;

/// Per-provider rank summary for a Prompt over the window.
#[derive(Debug, Clone)]
pub struct ProviderRank {
    pub provider: String,
    pub mean_rank: Option<f32>,
    /// Whether the brand was cited at all by this provider in the window.
    pub cited: bool,
}

#[derive(Debug, Clone)]
pub struct CompetitorRank {
    pub competitor: String,
    pub mean_rank: f32,
}

/// Aggregates for a single tracked Prompt over the evaluation window.
#[derive(Debug, Clone)]
pub struct PromptStat {
    pub prompt_id: Ulid,
    pub prompt: String,
    pub run_ids: Vec<Ulid>,
    pub citation_ids: Vec<Ulid>,
    pub n_runs_14d: u32,
    pub brand_mean_rank: Option<f32>,
    pub provider_ranks: Vec<ProviderRank>,
    pub competitor_ranks: Vec<CompetitorRank>,
    /// eTLD+1 domains the brand co-occurred with in citations for this Prompt.
    pub brand_citation_domains: Vec<String>,
    /// Competitor-aligned docs eTLD+1s present in this Prompt's citations (2.1).
    pub competitor_docs_present: Vec<String>,
}

/// Two-window citation-domain sets for the drift Kind (2.3).
#[derive(Debug, Clone)]
pub struct CitationDriftInput {
    pub prior_domains: Vec<String>,
    pub current_domains: Vec<String>,
    /// Precomputed: current top-3 co-occurring domains all rank lower in the
    /// benchmark `most_cited` than the prior window (the §2.3 second gate).
    pub current_top3_lower_benchmark_rank: bool,
    pub citation_ids: Vec<Ulid>,
}

/// A benchmark category slice (feeds 2.4 coverage-gap and 2.9 underperformance).
#[derive(Debug, Clone)]
pub struct BenchmarkCategory {
    pub category: String,
    pub brand_p_rank: f32,
    pub benchmark_p75: f32,
    /// Fraction of contributors' citation graphs the brand's eTLD+1 appears in.
    pub brand_in_graph_pct: f32,
    pub local_has_prompts: bool,
    /// Heuristic prompts from the static category template (NOT LLM-generated).
    pub suggested_prompts: Vec<String>,
    /// SHA-256 of the canonical benchmark aggregate query body.
    pub query_hash: String,
}

/// The full input bag for one generation run.
#[derive(Debug, Clone)]
pub struct EngineInput {
    pub project_id: Ulid,
    pub brand: String,
    pub brand_etld1: String,
    /// eTLD+1 of `brand.docs_url` (anseo.yaml v0.2), if configured.
    pub docs_etld1: Option<String>,
    pub competitors: Vec<String>,
    pub enabled_providers: Vec<String>,
    pub benchmark_opted_in: bool,
    pub extraction_p50: Option<f32>,
    /// Promotion threshold (default 0.6 — closes Phase 2 OQ-21).
    pub extraction_threshold: f32,
    pub extraction_suggested_plugin: String,
    pub prompts: Vec<PromptStat>,
    pub citation_drift: Option<CitationDriftInput>,
    pub benchmark_categories: Vec<BenchmarkCategory>,
    pub window: TimeWindow,
    /// Passed in (never `now()`) so output is byte-stable for the fixture.
    pub generated_at: DateTime<Utc>,
    pub engine_version: String,
    /// All Prompt Run ids in the window (evidence for meta-recs like 2.6).
    pub all_run_ids: Vec<Ulid>,
}
