//! Row structs for the Phase 1 tables, mirroring the migration's column
//! manifests verbatim (Story 1.3 AC-3).
//!
//! `id` and FK columns use the ULID newtypes from
//! [`anseo_core::ids`] directly — the `sqlx` feature on `anseo-core` brings
//! the `sqlx::Type`/`Encode`/`Decode` impls (AC-9), so `sqlx::query_as!`
//! decodes them without any call-site UUID conversion.
//!
//! `status` and `error_kind` are deliberately stored as `String` /
//! `Option<String>` here. The closed sets are enforced at the DB layer by the
//! `CHECK` constraints in the migration. Domain enums (`PromptRunStatus`,
//! typed `ProviderErrorKind` round-trip) are a Story 2.x concern; storage
//! must not pre-empt them.

use anseo_core::ids::{
    CitationId, ClaimId, GroundTruthFactId, MentionId, ProjectId, PromptId, PromptRunId,
};
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProjectRow {
    pub id: ProjectId,
    pub name: String,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// DB-authoritative brand config for a project. Kept separate from
/// [`ProjectRow`] so the many `ProjectRow` construction sites (tests, seeds)
/// stay untouched — the `variants`/`competitors` columns carry DB defaults.
#[derive(Debug, Clone)]
pub struct BrandRow {
    pub id: ProjectId,
    pub name: String,
    /// Brand-name variants/aliases.
    pub variants: Vec<String>,
    /// Competitor set as a JSONB array of `{ name, variants }` objects,
    /// mirroring `anseo_core::CompetitorConfig`.
    pub competitors: JsonValue,
    /// Optional URL of the brand's owned website.
    pub site_url: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptRow {
    pub id: PromptId,
    pub project_id: ProjectId,
    pub name: String,
    pub text: String,
    /// Free-form labels for grouping/rollups. AI-generated prompts carry
    /// "AUTO" when no existing tag is a better match.
    pub tags: Vec<String>,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptRunRow {
    pub id: PromptRunId,
    pub prompt_id: PromptId,
    pub provider: String,
    pub provider_model_version: String,
    pub provider_region: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub raw_response: JsonValue,
    pub request_parameters: JsonValue,
    pub status: String,
    pub error_kind: Option<String>,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MentionRow {
    pub id: MentionId,
    pub prompt_run_id: PromptRunId,
    pub entity: String,
    pub char_offset: i32,
    pub rank: i32,
    pub matched_text: String,
    pub sentiment_label: Option<String>,
    pub sentiment_score: Option<i16>,
    pub sentiment_lane: Option<String>,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CitationRow {
    pub id: CitationId,
    pub prompt_run_id: PromptRunId,
    pub url: Option<String>,
    pub domain: String,
    pub frequency: i32,
    pub source_type: Option<String>,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ExtractedClaimRow {
    pub id: ClaimId,
    pub prompt_run_id: PromptRunId,
    pub entity: String,
    pub claim_text: String,
    pub claim_kind: String,
    pub char_offset: Option<i32>,
    pub confidence: i16,
    pub extractor_lane: String,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GroundTruthFactRow {
    pub id: GroundTruthFactId,
    pub project_id: ProjectId,
    pub entity: String,
    pub fact_key: String,
    pub fact_value: String,
    pub source_url: Option<String>,
    pub source_label: Option<String>,
    pub source_type: Option<String>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    pub organization_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Summary row for the site-audit history list (Epic 32). The full report is
/// kept as JSONB in `audit_runs.report`; this struct carries the scalar
/// columns the history list renders.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditRunSummary {
    pub id: Uuid,
    pub target: String,
    pub overall_score: i16,
    pub pages_crawled: i32,
    pub gate_passed: Option<bool>,
    pub created_at: DateTime<Utc>,
}
