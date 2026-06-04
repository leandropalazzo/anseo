//! Analytics queries for the Dashboard surfaces (FR-17..FR-20) and Phase 2
//! anomaly detection (FR-26a).
//!
//! Phase 1 keeps every query in raw SQL so the API can call them directly
//! without a heavier ORM. Each function returns serde-friendly row structs.

pub mod anomaly;
pub mod citation_graph;
pub mod heatmap;
pub mod metrics_store;
pub mod sentiment;
pub mod volatility;

use chrono::{DateTime, Utc};
use opengeo_core::{ProjectId, PromptRunId};
use opengeo_storage::Storage;
use serde::{Deserialize, Serialize};
use sqlx::Row;

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

/// One cell of the overall visibility matrix: the brand's footprint for a
/// single (prompt × provider) pair over the window. `presence_rate` is the
/// share of successful runs that mentioned the brand; `avg_rank` is the mean
/// of each run's best mention rank (None when never mentioned).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibilityMatrixCell {
    pub prompt_name: String,
    pub provider: String,
    pub run_count: i64,
    pub mention_count: i64,
    pub presence_rate: f64,
    pub avg_rank: Option<f64>,
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

/// Story 14.2 input fetch — every (provider, domain) citation observed
/// within `days` days for this project. Feeds [`citation_graph::compute`].
pub async fn citation_graph_rows(
    storage: &Storage,
    project_id: ProjectId,
    days: i32,
) -> Result<Vec<citation_graph::CitationRow>, Error> {
    let days = days.clamp(1, 365);
    let interval = format!("{days} days");
    struct Raw {
        provider: String,
        domain: String,
        weight: i64,
    }
    // Aggregate in SQL so the API process only materializes the dedup'd
    // edge set, not every individual citation row. `frequency` is
    // collapsed via SUM so the graph weight matches the citation_summary
    // surface (FR-20 invariant: both surfaces report the same totals).
    let raw = sqlx::query_as!(
        Raw,
        r#"
        SELECT pr.provider_identity       AS "provider!: String",
               c.domain                   AS "domain!: String",
               SUM(c.frequency)::bigint   AS "weight!: i64"
        FROM citations c
        JOIN prompt_runs pr ON pr.id = c.prompt_run_id
        JOIN prompts p      ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - ($2::text)::interval
          AND pr.status     = 'ok'
        GROUP BY pr.provider_identity, c.domain
        ORDER BY SUM(c.frequency) DESC
        LIMIT 5000
        "#,
        project_id as ProjectId,
        interval,
    )
    .fetch_all(storage.pool())
    .await?;
    Ok(raw
        .into_iter()
        .map(|r| citation_graph::CitationRow {
            provider: r.provider,
            domain: r.domain,
            weight: r.weight.max(0) as u32,
        })
        .collect())
}

/// Story 14.3 input fetch — one row per (prompt_run × provider) within
/// `days`. The `rank` column comes from the brand's first mention in
/// each run if present; runs with no brand mention contribute
/// `rank = None` so [`heatmap::compute`] can compute presence rate
/// without re-querying.
pub async fn heatmap_rows(
    storage: &Storage,
    project_id: ProjectId,
    brand_entity: &str,
    days: i32,
) -> Result<Vec<heatmap::Sample>, Error> {
    let days = days.clamp(1, 365);
    let interval = format!("{days} days");
    struct Raw {
        date: chrono::NaiveDate,
        provider: String,
        rank: Option<i32>,
    }
    let raw = sqlx::query_as!(
        Raw,
        r#"
        SELECT
            (pr.started_at AT TIME ZONE 'UTC')::date AS "date!: chrono::NaiveDate",
            pr.provider_identity                     AS "provider!: String",
            (
                SELECT MIN(m.rank)
                FROM mentions m
                WHERE m.prompt_run_id = pr.id
                  AND m.entity        = $2
            )                                        AS "rank: i32"
        FROM prompt_runs pr
        JOIN prompts p ON p.id = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - ($3::text)::interval
          AND pr.started_at <= now()
          AND pr.status     = 'ok'
        ORDER BY pr.started_at DESC
        LIMIT 100000
        "#,
        project_id as ProjectId,
        brand_entity,
        interval,
    )
    .fetch_all(storage.pool())
    .await?;
    Ok(raw
        .into_iter()
        .map(|r| heatmap::Sample {
            date: r.date,
            provider: r.provider,
            rank: r.rank.map(|x| x as f64),
        })
        .collect())
}

/// Story 14.4 input fetch — per-day mean rank of `brand_entity` for one
/// (prompt × provider) over the trailing `window` days. Days with no
/// run produce no row; days with runs but no mention produce `None`.
/// Output is in chronological order, padded with `None` for any
/// missing days so the consumer can compute a fixed-length window.
pub async fn volatility_samples(
    storage: &Storage,
    project_id: ProjectId,
    prompt_name: &str,
    provider_name: &str,
    brand_entity: &str,
    window: u32,
) -> Result<Vec<Option<f64>>, Error> {
    // Clamp once; both the SQL interval AND the in-memory pad loop must
    // agree, otherwise an unclamped `window` (e.g. u32::MAX from a client)
    // would attempt an unbounded Vec allocation while the SQL only
    // returned 365 days.
    let window = window.clamp(1, 365);
    let days = window as i32;
    let interval = format!("{days} days");
    struct Raw {
        day: chrono::NaiveDate,
        avg_rank: Option<f64>,
    }
    let raw = sqlx::query_as!(
        Raw,
        r#"
        WITH per_run_rank AS (
            SELECT
                (pr.started_at AT TIME ZONE 'UTC')::date AS day,
                (
                    SELECT MIN(m.rank)
                    FROM mentions m
                    WHERE m.prompt_run_id = pr.id
                      AND m.entity        = $3
                ) AS rank
            FROM prompt_runs pr
            JOIN prompts p ON p.id = pr.prompt_id
            WHERE p.project_id = $1
              AND p.name       = $2
              AND pr.provider_identity = $4
              AND pr.started_at >= now() - ($5::text)::interval
              AND pr.started_at <= now()
              AND pr.status    = 'ok'
        )
        SELECT
            day      AS "day!: chrono::NaiveDate",
            AVG(rank)::double precision AS "avg_rank: f64"
        FROM per_run_rank
        GROUP BY day
        ORDER BY day
        "#,
        project_id as ProjectId,
        prompt_name,
        brand_entity,
        provider_name,
        interval,
    )
    .fetch_all(storage.pool())
    .await?;

    let observations: std::collections::BTreeMap<chrono::NaiveDate, Option<f64>> =
        raw.into_iter().map(|r| (r.day, r.avg_rank)).collect();
    let today = chrono::Utc::now().date_naive();
    let mut samples = Vec::with_capacity(window as usize);
    for offset in (0..window as i64).rev() {
        let day = today - chrono::Duration::days(offset);
        samples.push(observations.get(&day).copied().unwrap_or(None));
    }
    Ok(samples)
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
            SELECT pr.id, pr.provider_identity, pr.started_at
            FROM prompt_runs pr
            JOIN prompts p ON p.id = pr.prompt_id
            WHERE p.project_id = $1
              AND p.name       = $2
              AND pr.started_at >= now() - ($3::text)::interval
              AND pr.status    = 'ok'
        )
        SELECT
            date_trunc('day', wr.started_at) AS "bucket_start!: DateTime<Utc>",
            wr.provider_identity              AS "provider!: String",
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

/// Overall visibility matrix — one row per (prompt × provider) across ALL the
/// project's prompts, so operators see the whole footprint at a glance instead
/// of switching prompt-by-prompt. `brand_entity` is the primary brand whose
/// mentions define presence/rank. Raw query (no `.sqlx` cache entry needed).
pub async fn visibility_matrix(
    storage: &Storage,
    project_id: ProjectId,
    brand_entity: &str,
    days: i32,
) -> Result<Vec<VisibilityMatrixCell>, Error> {
    let days = days.clamp(1, 365);
    let interval = format!("{days} days");
    let rows = sqlx::query(
        r#"
        WITH run_rank AS (
            SELECT
                p.name               AS prompt_name,
                pr.provider_identity AS provider,
                (
                    SELECT MIN(m.rank)
                    FROM mentions m
                    WHERE m.prompt_run_id = pr.id
                      AND m.entity        = $2
                ) AS rank
            FROM prompt_runs pr
            JOIN prompts p ON p.id = pr.prompt_id
            WHERE p.project_id = $1
              AND pr.started_at >= now() - ($3::text)::interval
              AND pr.status    = 'ok'
        )
        SELECT
            prompt_name,
            provider,
            COUNT(*)::bigint                  AS run_count,
            COUNT(rank)::bigint               AS mention_count,
            AVG(rank)::double precision       AS avg_rank
        FROM run_rank
        GROUP BY prompt_name, provider
        ORDER BY prompt_name, provider
        "#,
    )
    .bind(project_id)
    .bind(brand_entity)
    .bind(interval)
    .fetch_all(storage.pool())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let run_count: i64 = r.try_get("run_count")?;
        let mention_count: i64 = r.try_get("mention_count")?;
        let avg_rank: Option<f64> = r.try_get("avg_rank")?;
        let presence_rate = if run_count > 0 {
            mention_count as f64 / run_count as f64
        } else {
            0.0
        };
        out.push(VisibilityMatrixCell {
            prompt_name: r.try_get("prompt_name")?,
            provider: r.try_get("provider")?,
            run_count,
            mention_count,
            presence_rate,
            avg_rank,
        });
    }
    Ok(out)
}

/// All-prompts visibility trend — presence/rank per (day × provider) summed
/// across every prompt in the project. Powers the aggregate trend heatmap that
/// complements [`visibility_matrix`]. Raw query (no `.sqlx` cache entry).
pub async fn visibility_trend_all(
    storage: &Storage,
    project_id: ProjectId,
    brand_entity: &str,
    days: i32,
) -> Result<Vec<VisibilityPoint>, Error> {
    let days = days.clamp(1, 365);
    let interval = format!("{days} days");
    let rows = sqlx::query(
        r#"
        WITH run_rank AS (
            SELECT
                date_trunc('day', pr.started_at) AS bucket,
                pr.provider_identity             AS provider,
                (
                    SELECT MIN(m.rank)
                    FROM mentions m
                    WHERE m.prompt_run_id = pr.id
                      AND m.entity        = $2
                ) AS rank
            FROM prompt_runs pr
            JOIN prompts p ON p.id = pr.prompt_id
            WHERE p.project_id = $1
              AND pr.started_at >= now() - ($3::text)::interval
              AND pr.status    = 'ok'
        )
        SELECT
            bucket                                                            AS bucket_start,
            provider,
            AVG(rank)::double precision                                       AS avg_rank,
            (COUNT(rank)::double precision / NULLIF(COUNT(*), 0)::double precision) AS presence_rate
        FROM run_rank
        GROUP BY bucket, provider
        ORDER BY bucket
        "#,
    )
    .bind(project_id)
    .bind(brand_entity)
    .bind(interval)
    .fetch_all(storage.pool())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(VisibilityPoint {
            bucket_start: r.try_get("bucket_start")?,
            provider: r.try_get("provider")?,
            avg_rank: r.try_get("avg_rank")?,
            presence_rate: r.try_get::<Option<f64>, _>("presence_rate")?.unwrap_or(0.0),
        });
    }
    Ok(out)
}

/// One hourly bucket of project-wide KPIs for the Overview sparklines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiTrendPoint {
    pub bucket_start: DateTime<Utc>,
    pub run_count: i64,
    /// Fraction in [0,1] of runs that succeeded in the bucket.
    pub success_rate: f64,
    /// Mean latency (ms) over successful runs reporting a duration; null when none.
    pub avg_latency_ms: Option<f64>,
}

/// Hourly project-wide KPI trend over the trailing `hours` window. Powers the
/// Overview "success rate / runs / latency" tile sparklines, which need a
/// per-bucket series rather than a single aggregate. Hourly (not daily) so a
/// single active day still yields a meaningful curve. Runtime query — no
/// `.sqlx` cache entry needed.
pub async fn kpi_trend(
    storage: &Storage,
    project_id: ProjectId,
    hours: i32,
) -> Result<Vec<KpiTrendPoint>, Error> {
    let hours = hours.clamp(1, 24 * 365);
    let interval = format!("{hours} hours");
    let rows = sqlx::query(
        r#"
        SELECT
            date_trunc('hour', pr.started_at)                            AS bucket_start,
            COUNT(pr.id)::bigint                                         AS run_count,
            SUM(CASE WHEN pr.status = 'ok' THEN 1 ELSE 0 END)::bigint    AS ok_count,
            AVG(
              CASE
                WHEN pr.status = 'ok' AND pr.finished_at IS NOT NULL
                THEN EXTRACT(EPOCH FROM (pr.finished_at - pr.started_at)) * 1000.0
                ELSE NULL
              END
            )::double precision                                          AS avg_latency_ms
        FROM prompt_runs pr
        JOIN prompts p ON p.id = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - ($2::text)::interval
        GROUP BY 1
        ORDER BY 1
        "#,
    )
    .bind(project_id)
    .bind(interval)
    .fetch_all(storage.pool())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let run_count: i64 = r.try_get("run_count")?;
        let ok_count: i64 = r.try_get("ok_count")?;
        out.push(KpiTrendPoint {
            bucket_start: r.try_get("bucket_start")?,
            run_count,
            success_rate: if run_count > 0 {
                ok_count as f64 / run_count as f64
            } else {
                0.0
            },
            avg_latency_ms: r.try_get("avg_latency_ms")?,
        });
    }
    Ok(out)
}

/// One hourly frequency point for a single citation domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationTrendPoint {
    pub bucket_start: DateTime<Utc>,
    pub frequency: i64,
}

/// Per-domain hourly citation-frequency trend for the project's top `limit`
/// domains over the trailing `hours` window. Powers the sparkline column in the
/// citations table. Returns a map of `domain -> ordered hourly points`. Runtime
/// query — no `.sqlx` cache entry needed.
pub async fn citation_trend(
    storage: &Storage,
    project_id: ProjectId,
    hours: i32,
    limit: i64,
) -> Result<std::collections::HashMap<String, Vec<CitationTrendPoint>>, Error> {
    let hours = hours.clamp(1, 24 * 365);
    let limit = limit.clamp(1, 500);
    let interval = format!("{hours} hours");
    let rows = sqlx::query(
        r#"
        WITH top_domains AS (
            SELECT c.domain
            FROM citations c
            JOIN prompt_runs pr ON pr.id = c.prompt_run_id
            JOIN prompts p      ON p.id  = pr.prompt_id
            WHERE p.project_id = $1
              AND pr.started_at >= now() - ($2::text)::interval
            GROUP BY c.domain
            ORDER BY SUM(c.frequency) DESC
            LIMIT $3
        )
        SELECT
            c.domain                          AS domain,
            date_trunc('hour', pr.started_at) AS bucket_start,
            SUM(c.frequency)::bigint          AS frequency
        FROM citations c
        JOIN prompt_runs pr ON pr.id = c.prompt_run_id
        JOIN prompts p      ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - ($2::text)::interval
          AND c.domain IN (SELECT domain FROM top_domains)
        GROUP BY c.domain, 2
        ORDER BY c.domain, 2
        "#,
    )
    .bind(project_id)
    .bind(interval)
    .bind(limit)
    .fetch_all(storage.pool())
    .await?;

    let mut out: std::collections::HashMap<String, Vec<CitationTrendPoint>> =
        std::collections::HashMap::new();
    for r in rows {
        let domain: String = r.try_get("domain")?;
        out.entry(domain).or_default().push(CitationTrendPoint {
            bucket_start: r.try_get("bucket_start")?,
            frequency: r.try_get("frequency")?,
        });
    }
    Ok(out)
}
