//! Analytics queries for the Dashboard surfaces (FR-17..FR-20).
//!
//! Phase 1 keeps every query in raw SQL so the API can call them directly
//! without a heavier ORM. Each function returns serde-friendly row structs.

use chrono::{DateTime, Utc};
use opengeo_core::{ProjectId, PromptRunId};
use opengeo_storage::Storage;
use serde::{Deserialize, Serialize};

pub use opengeo_storage::Error;

/// Paginated input for `list_runs`.
#[derive(Debug, Clone, Copy)]
pub struct RunListParams {
    pub limit: i64,
    pub offset: i64,
}

impl Default for RunListParams {
    fn default() -> Self {
        Self {
            limit: 25,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunListRow {
    /// ULID-form ID (matches what `PromptRunId::from_str()` round-trips).
    pub id: String,
    pub prompt_name: String,
    pub provider: String,
    pub provider_model_version: String,
    pub started_at: DateTime<Utc>,
    pub status: String,
    pub error_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationSummaryRow {
    pub domain: String,
    pub frequency: i64,
    pub source_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityPoint {
    pub bucket_start: DateTime<Utc>,
    pub provider: String,
    pub avg_rank: Option<f64>,
    pub presence_rate: f64,
}

/// Recent Prompt Runs across a Project (FR-17). Joined to `prompts` to surface
/// `prompt_name`; ordered newest-first.
pub async fn list_runs(
    storage: &Storage,
    project_id: ProjectId,
    params: RunListParams,
) -> Result<Vec<RunListRow>, Error> {
    let limit = params.limit.clamp(1, 500);
    let offset = params.offset.max(0);

    struct Raw {
        id: PromptRunId,
        prompt_name: String,
        provider: String,
        provider_model_version: String,
        started_at: DateTime<Utc>,
        status: String,
        error_kind: Option<String>,
    }

    let raw = sqlx::query_as!(
        Raw,
        r#"
        SELECT
            pr.id                             AS "id!: PromptRunId",
            p.name                            AS "prompt_name!: String",
            pr.provider                       AS "provider!: String",
            pr.provider_model_version         AS "provider_model_version!: String",
            pr.started_at                     AS "started_at!: DateTime<Utc>",
            pr.status                         AS "status!: String",
            pr.error_kind                     AS "error_kind: String"
        FROM prompt_runs pr
        JOIN prompts p ON p.id = pr.prompt_id
        WHERE p.project_id = $1
        ORDER BY pr.started_at DESC
        LIMIT $2 OFFSET $3
        "#,
        project_id as ProjectId,
        limit,
        offset,
    )
    .fetch_all(storage.pool())
    .await?;

    Ok(raw
        .into_iter()
        .map(|r| RunListRow {
            id: r.id.to_string(),
            prompt_name: r.prompt_name,
            provider: r.provider,
            provider_model_version: r.provider_model_version,
            started_at: r.started_at,
            status: r.status,
            error_kind: r.error_kind,
        })
        .collect())
}

/// Most frequent citation domains across a Project's runs (FR-20).
pub async fn citation_summary(
    storage: &Storage,
    project_id: ProjectId,
    limit: i64,
) -> Result<Vec<CitationSummaryRow>, Error> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as!(
        CitationSummaryRow,
        r#"
        SELECT
            c.domain                          AS "domain!: String",
            SUM(c.frequency)::bigint          AS "frequency!: i64",
            (
                SELECT c2.source_type
                FROM citations c2
                JOIN prompt_runs pr2 ON pr2.id = c2.prompt_run_id
                JOIN prompts p2      ON p2.id  = pr2.prompt_id
                WHERE p2.project_id = $1 AND c2.domain = c.domain
                GROUP BY c2.source_type
                ORDER BY COUNT(*) DESC
                LIMIT 1
            )                                 AS "source_type: String"
        FROM citations c
        JOIN prompt_runs pr ON pr.id = c.prompt_run_id
        JOIN prompts p      ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
        GROUP BY c.domain
        ORDER BY SUM(c.frequency) DESC
        LIMIT $2
        "#,
        project_id as ProjectId,
        limit,
    )
    .fetch_all(storage.pool())
    .await?;
    Ok(rows)
}

/// Visibility trend per Prompt × Provider × day (FR-19). Phase 1 surfaces
/// `presence_rate=1.0` for every successful bucket (ranking comes online
/// once Story 3.2 starts populating `mentions`); the SQL contract is stable.
pub async fn visibility_trend(
    storage: &Storage,
    project_id: ProjectId,
    prompt_name: &str,
    days: i32,
) -> Result<Vec<VisibilityPoint>, Error> {
    let days = days.clamp(1, 365);
    let interval = format!("{days} days");
    let rows = sqlx::query_as!(
        VisibilityPoint,
        r#"
        WITH window_runs AS (
            SELECT pr.id, pr.provider, pr.started_at
            FROM prompt_runs pr
            JOIN prompts p ON p.id = pr.prompt_id
            WHERE p.project_id = $1
              AND p.name       = $2
              AND pr.started_at >= now() - ($3::text)::interval
              AND pr.status    = 'ok'
        )
        SELECT
            date_trunc('day', wr.started_at) AS "bucket_start!: DateTime<Utc>",
            wr.provider                       AS "provider!: String",
            NULL::double precision            AS "avg_rank: f64",
            1.0::double precision             AS "presence_rate!: f64"
        FROM window_runs wr
        GROUP BY 1, 2
        ORDER BY 1
        "#,
        project_id as ProjectId,
        prompt_name,
        interval,
    )
    .fetch_all(storage.pool())
    .await?;
    Ok(rows)
}
