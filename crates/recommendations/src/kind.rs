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

#[cfg(test)]
mod tests {
    use super::*;
    use RecommendationKind::*;

    /// Every Kind in declaration order, so as_str/lane/serde assertions stay
    /// exhaustive: adding a Kind without updating this list fails the match in
    /// `as_str_matches_serde_snake_case` to compile, drawing attention.
    const ALL: [RecommendationKind; 10] = [
        DocsNotCitedForPrompt,
        CompetitorOutranksForPrompt,
        CitationDomainDrift,
        PromptCoverageGap,
        ProviderBlindspot,
        LowExtractionConfidence,
        BenchmarkCategoryUnderperformance,
        StructuralContentSuggestion,
        CitationQualityUplift,
        VolatilityAnomalyExplained,
    ];

    #[test]
    fn as_str_matches_serde_snake_case_for_every_kind() {
        // The wire identifier (`as_str`) MUST equal serde's snake_case rename for
        // every Kind — REST, storage and MCP all rely on the two agreeing.
        for k in ALL {
            let serde_name = serde_json::to_value(k).unwrap();
            assert_eq!(
                serde_json::Value::String(k.as_str().to_string()),
                serde_name,
                "as_str diverges from serde for {k:?}"
            );
        }
    }

    #[test]
    fn kinds_round_trip_through_serde_by_wire_name() {
        for k in ALL {
            let json = format!("\"{}\"", k.as_str());
            let back: RecommendationKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, k);
        }
    }

    #[test]
    fn lane_classification_is_exhaustive_and_correct() {
        // Deterministic lane: the seven Story 19.1 Kinds.
        for k in RecommendationKind::deterministic() {
            assert_eq!(
                k.lane(),
                Lane::Deterministic,
                "{k:?} should be deterministic"
            );
        }
        // LLM-aided lane (Story 19.3).
        assert_eq!(StructuralContentSuggestion.lane(), Lane::LlmAided);
        assert_eq!(CitationQualityUplift.lane(), Lane::LlmAided);
        // Hybrid lane.
        assert_eq!(VolatilityAnomalyExplained.lane(), Lane::Hybrid);
    }

    #[test]
    fn deterministic_set_is_exactly_the_deterministic_lane() {
        // The `deterministic()` convenience must contain ONLY deterministic-lane
        // Kinds and ALL of them — no LLM/hybrid Kind leaks in, none is dropped.
        let det = RecommendationKind::deterministic();
        assert_eq!(det.len(), 7);
        let det_count = ALL
            .iter()
            .filter(|k| k.lane() == Lane::Deterministic)
            .count();
        assert_eq!(det_count, 7, "exactly 7 Kinds are deterministic-lane");
        for k in det {
            assert_eq!(k.lane(), Lane::Deterministic);
        }
    }

    #[test]
    fn wire_names_are_unique() {
        // No two Kinds may share a wire identifier (would collide in storage).
        let mut names: Vec<&str> = ALL.iter().map(|k| k.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate Kind wire name");
    }

    #[test]
    fn lane_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&Lane::LlmAided).unwrap(),
            "\"llm_aided\""
        );
        assert_eq!(
            serde_json::to_string(&Lane::Deterministic).unwrap(),
            "\"deterministic\""
        );
        assert_eq!(serde_json::to_string(&Lane::Hybrid).unwrap(), "\"hybrid\"");
    }
}
