//! Row structs for the Phase 1 tables, mirroring the migration's column
//! manifests verbatim (Story 1.3 AC-3).
//!
//! `id` and FK columns use the ULID newtypes from
//! [`opengeo_core::ids`] directly — the `sqlx` feature on `opengeo-core` brings
//! the `sqlx::Type`/`Encode`/`Decode` impls (AC-9), so `sqlx::query_as!`
//! decodes them without any call-site UUID conversion.
//!
//! `status` and `error_kind` are deliberately stored as `String` /
//! `Option<String>` here. The closed sets are enforced at the DB layer by the
//! `CHECK` constraints in the migration. Domain enums (`PromptRunStatus`,
//! typed `ProviderErrorKind` round-trip) are a Story 2.x concern; storage
//! must not pre-empt them.

use chrono::{DateTime, Utc};
use opengeo_core::ids::{CitationId, MentionId, ProjectId, PromptId, PromptRunId};
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

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromptRow {
    pub id: PromptId,
    pub project_id: ProjectId,
    pub name: String,
    pub text: String,
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
