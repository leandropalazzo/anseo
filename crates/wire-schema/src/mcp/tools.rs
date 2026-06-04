//! Input + output DTOs for the 6 MCP tools (FR-46..FR-51).
//!
//! Source of truth: architecture-phase3-mcp-server.md §3. Field names + types
//! are verbatim from that doc. Any deviation is annotated with a `// SPEC:`
//! comment pointing to the relevant `OQ-MCP-*` open question.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::ProjectId;

// ============================================================================
// Shared sub-shapes
// ============================================================================

/// Time window vocabulary for trend/visibility/citation queries.
/// Matches the `/v1/.../?window=` query param vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    #[serde(rename = "7d")]
    SevenDays,
    #[serde(rename = "30d")]
    ThirtyDays,
    #[serde(rename = "all")]
    All,
}

/// Per-result ranking carried inside `run_prompt` results.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct Ranking {
    pub position: u32,
    pub total_candidates: u32,
}

/// Minimal mention record carried in MCP tool outputs.
///
/// SPEC NOTE: architecture-phase3-mcp-server.md §3.1 references
/// "MentionRecord from wire-schema, FR-3 shape". FR-3 DTOs do not yet live in
/// `crates/wire-schema` at the time Story 0.6 lands. The fields below are the
/// minimal extraction-stage shape PRD §6.3 commits to; they will be
/// reconciled with the FR-3 wire-schema struct when it lands (no rename
/// required — fields match by name).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct McpMentionRecord {
    pub subject: String,
    pub mention_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranking: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_confidence: Option<f32>,
}

/// Minimal citation record carried in MCP tool outputs.
///
/// SPEC NOTE: same reconciliation deferred as [`McpMentionRecord`] — FR-4
/// wire-schema struct will replace this when it lands.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct McpCitationRecord {
    pub url: String,
    pub domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_confidence: Option<f32>,
}

/// Per-provider per-result error, used inside `run_prompt` results when
/// `status != "ok"`. Architecturally distinct from `McpError`: this is a
/// *per-result* error embedded in a successful tool response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ResultError {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

// ============================================================================
// FR-46: run_prompt
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RunPromptInput {
    /// Required per AD-Phase3-MCP-ProjectScoping. Single-project servers
    /// default-resolve to the configured project name or the literal "default".
    pub project: ProjectId,
    /// Prompt name OR ULID.
    pub prompt: String,
    /// Optional — defaults to the project's configured provider set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<Vec<String>>,
    /// Optional ULID. If absent, the server generates one and forwards it as
    /// `Idempotency-Key:` to `POST /v1/prompt-runs`. Window: 24h (see
    /// OQ-MCP-3, default-if-undecided).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RunPromptOutput {
    pub prompt_id: String,
    pub results: Vec<RunPromptResult>,
    /// Always `true` for `run_prompt` — provider calls are non-deterministic.
    /// Surfaced so agent loops can decide whether to cache.
    pub non_deterministic_pipeline: bool,
    /// Correlation ID echoed from `X-OpenGEO-Request-Id`.
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RunPromptResult {
    pub prompt_run_id: String,
    pub provider: String,
    pub model: String,
    pub status: RunPromptStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranking: Option<Ranking>,
    pub mentions: Vec<McpMentionRecord>,
    pub citations: Vec<McpCitationRecord>,
    pub duration_ms: u64,
    /// Present only when `status != "ok"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ResultError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunPromptStatus {
    Ok,
    Failed,
    Partial,
    Capped,
}

// ============================================================================
// FR-47: get_visibility
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GetVisibilityInput {
    pub project: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<String>>,
    /// Optional; default `30d` (resolved server-side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GetVisibilityOutput {
    pub window: Window,
    pub series: Vec<VisibilitySeries>,
    /// `"project_has_no_prompts"` only; per-series empty reasons live on
    /// [`VisibilitySeriesSummary`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_reason: Option<String>,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct VisibilitySeries {
    pub prompt_id: String,
    pub prompt_name: String,
    pub points: Vec<VisibilityPoint>,
    pub summary: VisibilitySeriesSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct VisibilityPoint {
    /// ISO 8601 date `YYYY-MM-DD`.
    pub date: String,
    pub provider: String,
    pub visibility_score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranking: Option<u32>,
    pub mention_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct VisibilitySeriesSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_vs_prior_window: Option<f64>,
    /// `"no_prompt_runs_in_window"` or null.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty_reason: Option<String>,
}

// ============================================================================
// FR-48: compare_brands
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CompareBrandsInput {
    pub project: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CompareBrandsOutput {
    pub window: Window,
    pub brand: String,
    /// YAML declaration order — deterministic per §3.3 determinism contract.
    pub competitors: Vec<String>,
    pub rows: Vec<CompareBrandsRow>,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CompareBrandsRow {
    pub prompt_id: String,
    pub prompt_name: String,
    pub provider: String,
    /// Ordered `[brand, ...competitors_in_yaml_order]`.
    pub cells: Vec<CompareBrandsCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CompareBrandsCell {
    pub subject: String,
    /// `null` when subject absent (NOT omitted — §3.3 contract).
    pub ranking: Option<u32>,
    pub mention_count: u32,
}

// ============================================================================
// FR-49: get_citations
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct GetCitationsInput {
    pub project: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
    /// Optional; default 50; max 500 (validated server-side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_n: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GetCitationsOutput {
    pub window: Window,
    pub items: Vec<CitationSummaryItem>,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CitationSummaryItem {
    pub domain: String,
    pub frequency: u64,
    /// `documentation|blog|news|forum|other`. Free-form string for extractor-
    /// plugin headroom (§6.1).
    pub source_type: String,
    /// At most 5 sample prompt-run ULIDs.
    pub sample_prompt_run_ids: Vec<String>,
}

// ============================================================================
// FR-50: list_trends
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ListTrendsInput {
    pub project: ProjectId,
    pub window: Window,
    /// Optional [0,1]; default 0.3 (server-side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_significance: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ListTrendsOutput {
    pub window: Window,
    pub trends: Vec<TrendRecord>,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct TrendRecord {
    /// Free-form string for Analytics-plugin namespacing (`plugin:<name>:<kind>`).
    /// Built-ins: `threshold_regression`, `statistical_anomaly`,
    /// `response_change`. See architecture §6.1.
    pub trend_kind: String,
    pub prompt_id: String,
    pub prompt_name: String,
    pub provider: String,
    pub delta: TrendDelta,
    pub evidence_prompt_run_ids: Vec<String>,
    pub significance: f32,
    /// RFC 3339 UTC timestamp.
    pub detected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct TrendDelta {
    pub metric: String,
    pub from: f64,
    pub to: f64,
}

// ============================================================================
// FR-51: search_benchmarks (project-less per §4)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchBenchmarksInput {
    /// Free-text or structured category.
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Optional; default `30d` (server-side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<Window>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct SearchBenchmarksOutput {
    pub hits: Vec<BenchmarkHit>,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct BenchmarkHit {
    /// `fastest_growing_brands|most_cited_domains|recommendation_differences|<other>`.
    pub category: String,
    /// Always `"public_benchmark_dataset"` in Phase 3.
    pub source: String,
    /// Category-specific JSON shape (per architecture §3.6 + §11).
    pub finding: serde_json::Value,
    pub link_to_public_dashboard: String,
}

// ============================================================================
// FR-59 / Story 19.7: recommend.* tools
//
// The Recommendation envelope is the engine wire contract
// (architecture-phase3-geo-recommendations.md §4); the MCP surface consumes it
// verbatim as an opaque JSON object rather than re-modelling it, so the
// `tags`/`reproducibility` fields (incl. `non_deterministic_pipeline`, §5)
// reach the LLM client unchanged.
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecommendListInput {
    pub project: ProjectId,
    /// Optional; default 50, max 200 (validated server-side).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Opaque page cursor from a previous response's `next_cursor`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RecommendListOutput {
    /// Active recommendations, newest first; each item is the engine wire
    /// envelope verbatim (§4).
    pub recommendations: Vec<serde_json::Value>,
    /// `null` on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecommendShowInput {
    pub project: ProjectId,
    /// Recommendation ULID.
    pub recommendation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RecommendShowOutput {
    /// The engine wire envelope verbatim (§4), including full traceability.
    pub recommendation: serde_json::Value,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecommendAckInput {
    pub project: ProjectId,
    pub recommendation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecommendDismissInput {
    pub project: ProjectId,
    pub recommendation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RecommendMarkActedInput {
    pub project: ProjectId,
    pub recommendation_id: String,
    /// Optional evidence URL — the LLM client surfaces what changed. Decision
    /// L4 exposes both `evidence_url` and `note` as optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Shared output for the lifecycle-transition tools (`ack` / `dismiss` /
/// `mark_acted`): the updated envelope verbatim plus any lifecycle warnings
/// (e.g. `lifecycle.evidence_missing` from `mark_acted`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct RecommendTransitionOutput {
    pub recommendation: serde_json::Value,
    pub warnings: Vec<serde_json::Value>,
    pub trace_id: String,
}

// ============================================================================
// Roadmap Epic 32: audit — site citation-readiness (BYO-generation bridge).
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditInput {
    /// URL, sitemap URL, file:// URL, or local HTML fixture path to audit.
    pub target: String,
    /// Optional; default 25; clamped to [1, 200] server-side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pages: Option<u32>,
    /// Optional CI-gate thresholds: rule ids or severities (low/medium/high).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fail_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AuditOutput {
    pub target: String,
    pub overall_score: u8,
    pub pages_crawled: u32,
    pub findings: Vec<AuditFindingRecord>,
    /// Present only when `fail_on` was supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_passed: Option<bool>,
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct AuditFindingRecord {
    pub page_url: String,
    pub rule_id: String,
    /// `identity|extractability|corroboration`.
    pub category: String,
    /// `low|medium|high`.
    pub severity: String,
    /// `pass|warn|fail`.
    pub status: String,
    pub message: String,
}
