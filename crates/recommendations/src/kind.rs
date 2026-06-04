//! `RecommendationKind` taxonomy (architecture-phase3-geo-recommendations.md §2).
//!
//! New Kinds are additive (semver minor); removal is a major bump. Story 19.1
//! implements the seven **deterministic** Kinds; the LLM-aided / hybrid Kinds
//! (2.7, 2.8, 2.10) are declared here but produced by the LLM lane (Story 19.3).

use serde::{Deserialize, Serialize};

/// Which generation lane a Kind belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    /// Heuristic rules over aggregates. Byte-stable, no LLM.
    Deterministic,
    /// Invokes a Provider. Best-effort reproducibility.
    LlmAided,
    /// Deterministic core + optional LLM enrichment.
    Hybrid,
}

/// The starter-set taxonomy. Serializes to the canonical snake_case names
/// used on the wire, in storage, and over MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationKind {
    // ---- deterministic (Story 19.1) ----
    DocsNotCitedForPrompt,             // 2.1
    CompetitorOutranksForPrompt,       // 2.2
    CitationDomainDrift,               // 2.3
    PromptCoverageGap,                 // 2.4
    ProviderBlindspot,                 // 2.5
    LowExtractionConfidence,           // 2.6
    BenchmarkCategoryUnderperformance, // 2.9
    // ---- LLM-aided / hybrid (Story 19.3) ----
    StructuralContentSuggestion, // 2.7
    CitationQualityUplift,       // 2.8
    VolatilityAnomalyExplained,  // 2.10
}

impl RecommendationKind {
    /// Canonical wire identifier (matches serde's snake_case).
    pub fn as_str(&self) -> &'static str {
        use RecommendationKind::*;
        match self {
            DocsNotCitedForPrompt => "docs_not_cited_for_prompt",
            CompetitorOutranksForPrompt => "competitor_outranks_for_prompt",
            CitationDomainDrift => "citation_domain_drift",
            PromptCoverageGap => "prompt_coverage_gap",
            ProviderBlindspot => "provider_blindspot",
            LowExtractionConfidence => "low_extraction_confidence",
            BenchmarkCategoryUnderperformance => "benchmark_category_underperformance",
            StructuralContentSuggestion => "structural_content_suggestion",
            CitationQualityUplift => "citation_quality_uplift",
            VolatilityAnomalyExplained => "volatility_anomaly_explained",
        }
    }

    pub fn lane(&self) -> Lane {
        use RecommendationKind::*;
        match self {
            DocsNotCitedForPrompt
            | CompetitorOutranksForPrompt
            | CitationDomainDrift
            | PromptCoverageGap
            | ProviderBlindspot
            | LowExtractionConfidence
            | BenchmarkCategoryUnderperformance => Lane::Deterministic,
            StructuralContentSuggestion | CitationQualityUplift => Lane::LlmAided,
            VolatilityAnomalyExplained => Lane::Hybrid,
        }
    }

    /// The seven deterministic Kinds produced by Story 19.1.
    pub fn deterministic() -> [RecommendationKind; 7] {
        use RecommendationKind::*;
        [
            DocsNotCitedForPrompt,
            CompetitorOutranksForPrompt,
            CitationDomainDrift,
            PromptCoverageGap,
            ProviderBlindspot,
            LowExtractionConfidence,
            BenchmarkCategoryUnderperformance,
        ]
    }
}
