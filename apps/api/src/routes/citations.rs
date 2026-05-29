//! `GET /v1/citations/summary` — citation aggregation surface.
//!
//! Story 0.9 (Phase 3 epic-0 substrate) extends this endpoint with three
//! additive shapes plus three query filters:
//!
//! - Query: `days` (window in days; default 30, range [1, 365]),
//!   `provider` (filter to a single provider), `prompt` (filter to a
//!   single prompt by name). All three are optional.
//! - Response: `sample_prompt_run_ids` (≤ 5 ULIDs per domain — feeds the
//!   MCP `get_citations` tool per architecture-phase3-mcp-server §3.4),
//!   `provider_breakdown` (per-provider citation totals), `top_domains`
//!   (top-N domains ranked by frequency, mirrors `domains` with a stable
//!   cap so dashboards don't paginate), and `growth_rate` (window vs
//!   prior-equal-window delta — null if either side has zero citations).
//!
//! Backward compatibility:
//!   - `limit` query param still honored (defaults to 50, max 500).
//!   - `domains: [...]` array still present at the top level with the
//!     existing `domain` / `frequency` / `source_type` fields. New
//!     per-item fields (`sample_prompt_run_ids`) are additive.
//!   - All new top-level fields are additive; older SDK consumers that
//!     `#[serde(deny_unknown_fields)]` will break but the SDKs do not
//!     (verified against `crates/sdks/*`).
//!
//! `X-OpenGEO-Project` is accepted-but-ignored at this layer (Phase 2
//! single-project posture); the auth middleware resolves the project
//! from the API key.

use std::collections::BTreeMap;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opengeo_analytics::citation_summary;
use opengeo_core::ProjectId;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/citations/summary", get(summary))
}

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/citations/summary", get(summary))
}

#[derive(Debug, Deserialize)]
pub struct SummaryQuery {
    pub limit: Option<i64>,
    /// Window in days (default 30, clamped to [1, 365]).
    pub days: Option<i32>,
    /// Restrict citations to runs from this provider.
    pub provider: Option<String>,
    /// Restrict citations to runs against this prompt name.
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DomainEntry {
    pub domain: String,
    pub frequency: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Architecture §3.4 contract: ≤ 5 ULIDs per domain.
    pub sample_prompt_run_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderBreakdownEntry {
    pub provider: String,
    pub frequency: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SummaryResponse {
    /// Legacy field name preserved for backward compatibility. Same shape
    /// as v0.5.x but with the additive `sample_prompt_run_ids` field.
    pub domains: Vec<DomainEntry>,
    pub provider_breakdown: Vec<ProviderBreakdownEntry>,
    pub top_domains: Vec<DomainEntry>,
    /// Ratio of current-window total / prior-equal-window total minus 1.
    /// Null when the prior window had zero citations (avoids div-by-zero
    /// and "+∞%" spikes in the dashboard).
    pub growth_rate: Option<f64>,
    /// Effective window in days the response was computed over. Echoed
    /// so clients can render "Last 30 days" labels without re-deriving.
    pub window_days: i32,
}

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 500;
const DEFAULT_DAYS: i32 = 30;
const MAX_DAYS: i32 = 365;
const SAMPLE_RUN_CAP: usize = 5;

async fn summary(
    State(state): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> Result<Json<SummaryResponse>, StatusCode> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let days = q.days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS);

    // When no filters supplied, fall back to the legacy `citation_summary`
    // query so the existing `.sqlx` cache + ordering invariants stay
    // intact. The enriched fields are computed by the helpers below
    // either way.
    let legacy_rows = citation_summary(&state.storage, state.project_id, limit)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "citation summary failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let filtered = fetch_filtered_rows(
        &state.storage,
        state.project_id,
        days,
        q.provider.as_deref(),
        q.prompt.as_deref(),
        limit,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "citation summary filtered fetch failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // The `domains` (legacy) field uses the filtered set when any
    // filter is supplied, else the legacy unfiltered set. This keeps
    // the v0.5.x semantics for callers that pass only `limit`.
    let any_filter = q.provider.is_some() || q.prompt.is_some() || q.days.is_some();
    let domains: Vec<DomainEntry> = if any_filter {
        filtered.clone()
    } else {
        legacy_rows
            .iter()
            .map(|r| DomainEntry {
                domain: r.domain.clone(),
                frequency: r.frequency,
                source_type: r.source_type.clone(),
                sample_prompt_run_ids: Vec::new(),
            })
            .collect()
    };

    // Always populate sample_prompt_run_ids on legacy-mode `domains` by
    // looking them up via the same helper (capped at 5 each).
    let domains = enrich_sample_ids(&state.storage, state.project_id, days, domains).await;

    let provider_breakdown = fetch_provider_breakdown(
        &state.storage,
        state.project_id,
        days,
        q.prompt.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "citation summary provider breakdown failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let top_domains = filtered.clone();

    let growth_rate = compute_growth_rate(
        &state.storage,
        state.project_id,
        days,
        q.provider.as_deref(),
        q.prompt.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "citation summary growth rate failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(SummaryResponse {
        domains,
        provider_breakdown,
        top_domains,
        growth_rate,
        window_days: days,
    }))
}

async fn fetch_filtered_rows(
    storage: &opengeo_storage::Storage,
    project_id: ProjectId,
    days: i32,
    provider: Option<&str>,
    prompt: Option<&str>,
    limit: i64,
) -> Result<Vec<DomainEntry>, sqlx::Error> {
    let interval = format!("{days} days");
    let provider_opt: Option<String> = provider.map(|s| s.to_string());
    let prompt_opt: Option<String> = prompt.map(|s| s.to_string());
    // Aggregate (domain, source_type) and collect up to 5 sample run ids.
    let rows = sqlx::query(
        r#"
        WITH window_runs AS (
            SELECT pr.id, pr.provider, p.name AS prompt_name
            FROM prompt_runs pr
            JOIN prompts p ON p.id = pr.prompt_id
            WHERE p.project_id = $1
              AND pr.started_at >= now() - ($2::text)::interval
              AND ($3::text IS NULL OR pr.provider = $3)
              AND ($4::text IS NULL OR p.name = $4)
        )
        SELECT
            c.domain                                AS domain,
            SUM(c.frequency)::bigint                AS frequency,
            (
                SELECT c2.source_type
                FROM citations c2
                WHERE c2.prompt_run_id IN (SELECT id FROM window_runs)
                  AND c2.domain = c.domain
                  AND c2.source_type IS NOT NULL
                GROUP BY c2.source_type
                ORDER BY COUNT(*) DESC
                LIMIT 1
            )                                       AS source_type,
            (
                SELECT array_agg(DISTINCT pr_id::text)
                FROM (
                    SELECT c3.prompt_run_id AS pr_id
                    FROM citations c3
                    WHERE c3.prompt_run_id IN (SELECT id FROM window_runs)
                      AND c3.domain = c.domain
                    ORDER BY c3.prompt_run_id
                    LIMIT 5
                ) s
            )                                       AS sample_ids
        FROM citations c
        WHERE c.prompt_run_id IN (SELECT id FROM window_runs)
        GROUP BY c.domain
        ORDER BY SUM(c.frequency) DESC
        LIMIT $5
        "#,
    )
    .bind(project_id)
    .bind(interval)
    .bind(provider_opt)
    .bind(prompt_opt)
    .bind(limit)
    .fetch_all(storage.pool())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let domain: String = r.try_get("domain")?;
        let frequency: i64 = r.try_get("frequency")?;
        let source_type: Option<String> = r.try_get("source_type")?;
        let raw_ids: Option<Vec<String>> = r.try_get("sample_ids")?;
        let sample_prompt_run_ids = raw_ids
            .unwrap_or_default()
            .into_iter()
            .take(SAMPLE_RUN_CAP)
            .collect();
        out.push(DomainEntry {
            domain,
            frequency,
            source_type,
            sample_prompt_run_ids,
        });
    }
    Ok(out)
}

/// Re-fetches sample_prompt_run_ids for the (unfiltered) legacy domains list
/// so MCP `get_citations` can consume the same shape regardless of which
/// branch produced the `domains` array.
async fn enrich_sample_ids(
    storage: &opengeo_storage::Storage,
    project_id: ProjectId,
    days: i32,
    mut entries: Vec<DomainEntry>,
) -> Vec<DomainEntry> {
    if entries.iter().all(|e| !e.sample_prompt_run_ids.is_empty()) {
        return entries;
    }
    let interval = format!("{days} days");
    let domains: Vec<String> = entries.iter().map(|e| e.domain.clone()).collect();
    if domains.is_empty() {
        return entries;
    }
    let res = sqlx::query(
        r#"
        SELECT c.domain                AS domain,
               c.prompt_run_id::text   AS prompt_run_id
        FROM citations c
        JOIN prompt_runs pr ON pr.id = c.prompt_run_id
        JOIN prompts     p  ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - ($2::text)::interval
          AND c.domain = ANY($3)
        ORDER BY c.domain, c.prompt_run_id
        "#,
    )
    .bind(project_id)
    .bind(interval)
    .bind(domains.clone())
    .fetch_all(storage.pool())
    .await;

    let Ok(rows) = res else {
        return entries;
    };
    let mut by_domain: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for r in rows {
        let domain: String = match r.try_get("domain") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let pr_id: String = match r.try_get("prompt_run_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let bucket = by_domain.entry(domain).or_default();
        if bucket.len() < SAMPLE_RUN_CAP && !bucket.contains(&pr_id) {
            bucket.push(pr_id);
        }
    }
    for e in entries.iter_mut() {
        if e.sample_prompt_run_ids.is_empty() {
            if let Some(ids) = by_domain.remove(&e.domain) {
                e.sample_prompt_run_ids = ids;
            }
        }
    }
    entries
}

async fn fetch_provider_breakdown(
    storage: &opengeo_storage::Storage,
    project_id: ProjectId,
    days: i32,
    prompt: Option<&str>,
) -> Result<Vec<ProviderBreakdownEntry>, sqlx::Error> {
    let interval = format!("{days} days");
    let prompt_opt: Option<String> = prompt.map(|s| s.to_string());
    let rows = sqlx::query(
        r#"
        SELECT pr.provider              AS provider,
               SUM(c.frequency)::bigint AS frequency
        FROM citations c
        JOIN prompt_runs pr ON pr.id = c.prompt_run_id
        JOIN prompts     p  ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - ($2::text)::interval
          AND ($3::text IS NULL OR p.name = $3)
        GROUP BY pr.provider
        ORDER BY SUM(c.frequency) DESC
        "#,
    )
    .bind(project_id)
    .bind(interval)
    .bind(prompt_opt)
    .fetch_all(storage.pool())
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(ProviderBreakdownEntry {
                provider: r.try_get("provider")?,
                frequency: r.try_get("frequency")?,
            })
        })
        .collect()
}

async fn compute_growth_rate(
    storage: &opengeo_storage::Storage,
    project_id: ProjectId,
    days: i32,
    provider: Option<&str>,
    prompt: Option<&str>,
) -> Result<Option<f64>, sqlx::Error> {
    let curr_interval = format!("{days} days");
    let prior_lower = format!("{} days", days * 2);
    let provider_opt: Option<String> = provider.map(|s| s.to_string());
    let prompt_opt: Option<String> = prompt.map(|s| s.to_string());
    let row = sqlx::query(
        r#"
        WITH curr AS (
            SELECT COALESCE(SUM(c.frequency), 0)::bigint AS total
            FROM citations c
            JOIN prompt_runs pr ON pr.id = c.prompt_run_id
            JOIN prompts     p  ON p.id  = pr.prompt_id
            WHERE p.project_id = $1
              AND pr.started_at >= now() - ($2::text)::interval
              AND ($4::text IS NULL OR pr.provider = $4)
              AND ($5::text IS NULL OR p.name = $5)
        ),
        prior AS (
            SELECT COALESCE(SUM(c.frequency), 0)::bigint AS total
            FROM citations c
            JOIN prompt_runs pr ON pr.id = c.prompt_run_id
            JOIN prompts     p  ON p.id  = pr.prompt_id
            WHERE p.project_id = $1
              AND pr.started_at >= now() - ($3::text)::interval
              AND pr.started_at <  now() - ($2::text)::interval
              AND ($4::text IS NULL OR pr.provider = $4)
              AND ($5::text IS NULL OR p.name = $5)
        )
        SELECT (SELECT total FROM curr)  AS curr_total,
               (SELECT total FROM prior) AS prior_total
        "#,
    )
    .bind(project_id)
    .bind(curr_interval)
    .bind(prior_lower)
    .bind(provider_opt)
    .bind(prompt_opt)
    .fetch_one(storage.pool())
    .await?;
    let curr: i64 = row.try_get("curr_total")?;
    let prior: i64 = row.try_get("prior_total")?;
    if prior == 0 {
        return Ok(None);
    }
    Ok(Some((curr as f64 - prior as f64) / prior as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_response_serializes_with_required_fields() {
        let resp = SummaryResponse {
            domains: vec![DomainEntry {
                domain: "example.com".into(),
                frequency: 3,
                source_type: Some("blog".into()),
                sample_prompt_run_ids: vec!["01HXYZ".into()],
            }],
            provider_breakdown: vec![ProviderBreakdownEntry {
                provider: "openai".into(),
                frequency: 3,
            }],
            top_domains: vec![],
            growth_rate: Some(0.25),
            window_days: 30,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v["domains"].is_array());
        assert!(v["provider_breakdown"].is_array());
        assert!(v["top_domains"].is_array());
        assert_eq!(v["window_days"], 30);
        assert_eq!(v["growth_rate"], serde_json::json!(0.25));
        // Backward-compat: legacy `domains[].domain` + `frequency` present.
        assert_eq!(v["domains"][0]["domain"], "example.com");
        assert_eq!(v["domains"][0]["frequency"], 3);
        // Additive: sample_prompt_run_ids is always present (possibly empty).
        assert!(v["domains"][0]["sample_prompt_run_ids"].is_array());
    }

    #[test]
    fn summary_query_round_trips_through_serde_json() {
        // Pure shape test — confirms the Deserialize impl honors all four
        // optional fields. `Query<T>` uses serde_urlencoded at runtime;
        // a json roundtrip exercises the same `Deserialize` derive.
        let raw = serde_json::json!({
            "limit": 10,
            "days": 14,
            "provider": "openai",
            "prompt": "vector-db",
        });
        let q: SummaryQuery = serde_json::from_value(raw).unwrap();
        assert_eq!(q.days, Some(14));
        assert_eq!(q.provider.as_deref(), Some("openai"));
        assert_eq!(q.prompt.as_deref(), Some("vector-db"));
        assert_eq!(q.limit, Some(10));
    }

    #[test]
    fn summary_query_accepts_empty() {
        let q: SummaryQuery = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(q.days, None);
        assert!(q.provider.is_none());
        assert!(q.prompt.is_none());
        assert!(q.limit.is_none());
    }

    #[test]
    fn growth_rate_is_omittable_via_null() {
        let resp = SummaryResponse {
            domains: vec![],
            provider_breakdown: vec![],
            top_domains: vec![],
            growth_rate: None,
            window_days: 7,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v["growth_rate"].is_null());
    }
}
