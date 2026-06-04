//! Common Recommendation envelope (architecture-phase3-geo-recommendations.md §4).
//!
//! Every Recommendation — regardless of Kind or lane — carries this shape.
//! Serialized identically over REST (§8) and MCP. Field order is the struct
//! order; `payload` is a `serde_json::Value` whose object keys serialize
//! sorted — both deterministic, which is what the `[rec-1]` byte-stable
//! contract relies on.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::kind::RecommendationKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    /// Freshly generated; not yet acknowledged (§6). Lifecycle transitions
    /// are owned by Story 19.4.
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproducibilityClass {
    ByteStable,
    BestEffort,
    NonDeterministic,
}

/// The binding tag that marks a Recommendation's reproducibility as best-effort
/// (§5). Invariant: present in `tags` **iff** `reproducibility.class ==
/// NonDeterministic`.
pub const TAG_NON_DETERMINISTIC: &str = "non_deterministic_pipeline";
pub const TAG_DETERMINISTIC_LANE: &str = "deterministic_lane";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkQueryRef {
    /// Canonical aggregate name, e.g. `recommendation-differences`.
    pub name: String,
    /// SHA-256 of the canonicalized aggregate query body.
    pub query_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reproducibility {
    pub class: ReproducibilityClass,
    /// Why the class is what it is. `None` for the deterministic lane.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// LLM provenance. `None` for the deterministic lane (Story 19.1 never
/// populates it). Declared here so Story 19.3 lands non-breaking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmTrace {
    pub provider: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_content_hash: Option<String>,
    pub prompt_template_id: String,
    pub prompt_template_version: String,
    pub full_prompt: String,
    pub full_response: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    pub latency_ms: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

/// The committed traceability obligation (PRD line 623). Empty traceability is
/// a hard bug — `[rec-6]` asserts every Recommendation carries real evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Traceability {
    pub source_run_ids: Vec<Ulid>,
    pub source_run_ids_truncated: bool,
    pub source_citation_ids: Vec<Ulid>,
    pub source_citation_ids_truncated: bool,
    pub source_benchmark_queries: Vec<BenchmarkQueryRef>,
    pub window: TimeWindow,
    /// SHA-256 of the canonical-JSON inputs that fed the generator. Enables
    /// byte-stable replay assertion for the deterministic lane.
    pub input_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm: Option<LlmTrace>,
}

impl Traceability {
    /// `[rec-6]`: a Recommendation must carry *some* evidence — at minimum a
    /// fingerprint + window, plus at least one source list populated.
    pub fn is_non_empty(&self) -> bool {
        !self.input_fingerprint.is_empty()
            && (!self.source_run_ids.is_empty()
                || !self.source_citation_ids.is_empty()
                || !self.source_benchmark_queries.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: Ulid,
    pub project_id: Ulid,
    pub kind: RecommendationKind,
    pub severity: Severity,
    pub confidence_band: ConfidenceBand,
    pub state: LifecycleState,
    /// Human-readable summary, ≤ 240 chars. Deterministic for the
    /// deterministic lane.
    pub summary: String,
    pub payload: serde_json::Value,
    pub traceability: Traceability,
    pub reproducibility: Reproducibility,
    /// `non_deterministic_pipeline` is the single binding tag (§5).
    pub tags: Vec<String>,
    pub generated_at: DateTime<Utc>,
    /// Engine semver, pins the deterministic output.
    pub engine_version: String,
}

impl Recommendation {
    /// §5 invariant: `class == NonDeterministic` **iff** tags contains
    /// `non_deterministic_pipeline`.
    pub fn reproducibility_invariant_holds(&self) -> bool {
        let has_tag = self.tags.iter().any(|t| t == TAG_NON_DETERMINISTIC);
        let is_nd = self.reproducibility.class == ReproducibilityClass::NonDeterministic;
        has_tag == is_nd
    }
}
