#![allow(clippy::doc_overindented_list_items)]
//! Phase 3 Story 0.8 — `GET /v1/comparisons` substrate endpoint (FR-48 substrate).
//!
//! Deterministic brand-vs-competitors comparison matrix consumed by the
//! MCP `compare_brands` tool (architecture-phase3-mcp-server.md §3.3).
//!
//! Query params:
//! - `brands`    — comma-separated, 2..=6 entries (required). The first
//!                 entry is treated as the subject "brand"; remainder are
//!                 "competitors", in caller-declared order. Validation:
//!                 returns 400 outside the [2,6] range or with empty entries.
//! - `prompts`   — comma-separated prompt names (optional; defaults to all
//!                 prompts for the project).
//! - `providers` — comma-separated provider names (optional; defaults to
//!                 all providers observed in the window).
//! - `window`    — `1d` | `7d` | `30d` (default `7d`).
//!
//! Response shape (re-exported from `opengeo_wire_schema::mcp::tools`):
//! `CompareBrandsOutput` — `{ window, brand, competitors, rows, trace_id }`.
//! Rows are ordered `(prompt_name ASC, provider ASC)`; cells are ordered
//! `[brand, ...competitors_in_caller_order]` with `ranking: null` (NOT
//! omitted) when a subject is absent. Determinism contract per §3.3.
//!
//! `X-OpenGEO-Project` header is accepted-but-ignored per L2
//! (`AD-Phase3-ProjectScopingUnified`).

use std::collections::BTreeMap;

use axum::extract::{Extension, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::get;
use axum::{Json, Router};
use opengeo_core::ProjectId;
use opengeo_wire_schema::mcp::tools::{
    CompareBrandsCell, CompareBrandsOutput, CompareBrandsRow, Window,
};
use serde::Deserialize;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/comparisons", get(comparisons_handler))
}

#[derive(Debug, Deserialize)]
pub struct ComparisonsQuery {
    /// Comma-separated list, 2..=6 entries. First entry is the subject brand.
    pub brands: String,
    /// Optional comma-separated prompt names.
    pub prompts: Option<String>,
    /// Optional comma-separated provider names.
    pub providers: Option<String>,
    /// `1d` | `7d` | `30d`. Default `7d`.
    pub window: Option<String>,
}

fn err_body(
    status: StatusCode,
    error: &str,
    message: &str,
) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({
            "error": error,
            "message": message,
        })),
    )
}

/// Split a comma-separated query value, trim entries, drop empties.
fn split_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Map `1d|7d|30d` → days. The wire-schema `Window` enum lacks `1d` (its
/// vocabulary is `7d|30d|all`), so we keep this endpoint's window resolved
/// internally as days and clamp the wire output to the closest matching
/// `Window` variant for serialization. `1d` is widened to `Window::SevenDays`
/// for the wire field to honor the `CompareBrandsOutput` shape; the
/// `window_days` info is faithfully reflected in the row data itself.
fn parse_window(raw: Option<&str>) -> Result<(i32, Window), (StatusCode, Json<serde_json::Value>)>
{
    match raw.unwrap_or("7d") {
        "1d" => Ok((1, Window::SevenDays)),
        "7d" => Ok((7, Window::SevenDays)),
        "30d" => Ok((30, Window::ThirtyDays)),
        other => Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_window",
            &format!("`window` must be one of 1d|7d|30d (got `{other}`)"),
        )),
    }
}

/// Pull the `X-OpenGEO-Request-Id` header if present, else mint a fresh
/// ULID. Mirrors the trace_id convention used elsewhere in the v1 surface.
fn resolve_trace_id(headers: &HeaderMap) -> String {
    headers
        .get("x-opengeo-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| ulid::Ulid::new().to_string())
}

async fn comparisons_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ComparisonsQuery>,
) -> Result<Json<CompareBrandsOutput>, (StatusCode, Json<serde_json::Value>)> {
    // ---- Validate `brands` (2..=6) -----------------------------------------
    let brands = split_csv(&q.brands);
    if brands.len() < 2 || brands.len() > 6 {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_brands",
            "`brands` must be a comma-separated list of 2..=6 non-empty entries",
        ));
    }
    // First entry = subject brand; remainder = competitors in caller order.
    let brand = brands[0].clone();
    let competitors: Vec<String> = brands[1..].to_vec();
    let subjects = brands.clone(); // cell-iteration order: [brand, ...competitors]

    let prompt_filter: Option<Vec<String>> = q.prompts.as_deref().map(split_csv);
    if matches!(prompt_filter.as_ref(), Some(v) if v.is_empty()) {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_prompts",
            "`prompts` was provided but contained no non-empty entries",
        ));
    }
    let provider_filter: Option<Vec<String>> = q.providers.as_deref().map(split_csv);
    if matches!(provider_filter.as_ref(), Some(v) if v.is_empty()) {
        return Err(err_body(
            StatusCode::BAD_REQUEST,
            "invalid_providers",
            "`providers` was provided but contained no non-empty entries",
        ));
    }

    let (window_days, window_wire) = parse_window(q.window.as_deref())?;

    let trace_id = resolve_trace_id(&headers);

    // ---- Fetch raw (prompt_name, prompt_id, provider, entity, ranking, count) ----
    let rows = fetch_comparison_rows(
        &state.storage,
        project_id,
        window_days,
        prompt_filter.as_deref(),
        provider_filter.as_deref(),
        &subjects,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, route = "comparisons", "fetch failed");
        err_body(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "comparisons fetch failed",
        )
    })?;

    // ---- Assemble matrix (deterministic order) -----------------------------
    // Group rows by (prompt_name, provider). BTreeMap → deterministic
    // ordering by `(prompt_name ASC, provider ASC)` per §3.3 contract.
    #[derive(Default)]
    struct Bucket {
        prompt_id: String,
        per_subject: BTreeMap<String, (Option<u32>, u32)>,
    }
    let mut groups: BTreeMap<(String, String), Bucket> = BTreeMap::new();
    for r in rows {
        let key = (r.prompt_name.clone(), r.provider.clone());
        let entry = groups.entry(key).or_default();
        entry.prompt_id = r.prompt_id;
        // Only retain subjects we asked about; the SQL already filtered, but
        // defensive in case a subject string mismatches.
        if subjects.iter().any(|s| s == &r.entity) {
            // Aggregate across mentions: best (lowest) rank wins, sum count.
            let slot = entry.per_subject.entry(r.entity).or_insert((None, 0));
            slot.0 = match (slot.0, r.ranking) {
                (None, b) => b,
                (a, None) => a,
                (Some(x), Some(y)) => Some(x.min(y)),
            };
            slot.1 = slot.1.saturating_add(r.mention_count);
        }
    }

    let mut out_rows: Vec<CompareBrandsRow> = Vec::with_capacity(groups.len());
    for ((prompt_name, provider), bucket) in groups {
        let cells: Vec<CompareBrandsCell> = subjects
            .iter()
            .map(|subj| {
                let (ranking, mention_count) =
                    bucket.per_subject.get(subj).copied().unwrap_or((None, 0));
                CompareBrandsCell {
                    subject: subj.clone(),
                    ranking,
                    mention_count,
                }
            })
            .collect();
        out_rows.push(CompareBrandsRow {
            prompt_id: bucket.prompt_id,
            prompt_name,
            provider,
            cells,
        });
    }

    Ok(Json(CompareBrandsOutput {
        window: window_wire,
        brand,
        competitors,
        rows: out_rows,
        trace_id,
    }))
}

// ============================================================================
// DB layer
// ============================================================================

struct RawComparisonRow {
    prompt_id: String,
    prompt_name: String,
    provider: String,
    entity: String,
    ranking: Option<u32>,
    mention_count: u32,
}

/// Pull one row per (prompt × provider × entity) within the window. Aggregates
/// rank as MIN (best-rank-wins) and mention occurrences as COUNT.
async fn fetch_comparison_rows(
    storage: &opengeo_storage::Storage,
    project_id: ProjectId,
    window_days: i32,
    prompt_filter: Option<&[String]>,
    provider_filter: Option<&[String]>,
    subjects: &[String],
) -> Result<Vec<RawComparisonRow>, opengeo_storage::Error> {
    let days = window_days.clamp(1, 365);
    let interval = format!("{days} days");

    // Always pass arrays — `cardinality = 0` means "no filter". Keeps the
    // statement plan stable regardless of which optional knobs are set.
    let prompt_vec: Vec<String> = prompt_filter.map(|v| v.to_vec()).unwrap_or_default();
    let provider_vec: Vec<String> = provider_filter.map(|v| v.to_vec()).unwrap_or_default();
    let subjects_vec: Vec<String> = subjects.to_vec();

    // Hand-rolled `query()` (vs. `query_as!`) because the optional-array
    // filter idiom + sqlx-prepare cache reproducibility is simpler with
    // `bind()` than with the macro form. Mirrors the pattern in
    // `apps/api/src/routes/anomalies.rs`.
    use sqlx::Row;
    let rows = sqlx::query(
        r#"
        SELECT
            p.id                                  AS prompt_id,
            p.name                                AS prompt_name,
            pr.provider                           AS provider,
            m.entity                              AS entity,
            MIN(m.rank)::int                      AS ranking,
            COUNT(*)::bigint                      AS mention_count
        FROM mentions m
        JOIN prompt_runs pr ON pr.id = m.prompt_run_id
        JOIN prompts p      ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - ($2::text)::interval
          AND pr.status     = 'ok'
          AND m.entity      = ANY($3::text[])
          AND (cardinality($4::text[]) = 0 OR p.name      = ANY($4::text[]))
          AND (cardinality($5::text[]) = 0 OR pr.provider = ANY($5::text[]))
        GROUP BY p.id, p.name, pr.provider, m.entity
        "#,
    )
    .bind(project_id)
    .bind(interval)
    .bind(&subjects_vec)
    .bind(&prompt_vec)
    .bind(&provider_vec)
    .fetch_all(storage.pool())
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let prompt_id: uuid::Uuid = row.try_get("prompt_id")?;
        let prompt_name: String = row.try_get("prompt_name")?;
        let provider: String = row.try_get("provider")?;
        let entity: String = row.try_get("entity")?;
        let ranking: Option<i32> = row.try_get("ranking")?;
        let mention_count: i64 = row.try_get("mention_count")?;
        out.push(RawComparisonRow {
            prompt_id: prompt_id.to_string(),
            prompt_name,
            provider,
            entity,
            ranking: ranking.and_then(|x| u32::try_from(x).ok()),
            mention_count: u32::try_from(mention_count.max(0)).unwrap_or(u32::MAX),
        });
    }
    Ok(out)
}

// ============================================================================
// Unit tests (no DB)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_csv_drops_empties_and_trims() {
        assert_eq!(
            split_csv("a, b ,, c "),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert!(split_csv("").is_empty());
        assert!(split_csv(",, ,").is_empty());
    }

    #[test]
    fn parse_window_defaults_to_7d() {
        let (days, w) = parse_window(None).unwrap();
        assert_eq!(days, 7);
        assert!(matches!(w, Window::SevenDays));
    }

    #[test]
    fn parse_window_accepts_1d_7d_30d() {
        assert_eq!(parse_window(Some("1d")).unwrap().0, 1);
        assert_eq!(parse_window(Some("7d")).unwrap().0, 7);
        assert_eq!(parse_window(Some("30d")).unwrap().0, 30);
    }

    #[test]
    fn parse_window_rejects_bogus() {
        let err = parse_window(Some("90d")).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn resolve_trace_id_prefers_header() {
        let mut h = HeaderMap::new();
        h.insert("x-opengeo-request-id", "abc-123".parse().unwrap());
        assert_eq!(resolve_trace_id(&h), "abc-123");
    }

    #[test]
    fn resolve_trace_id_mints_when_missing() {
        let h = HeaderMap::new();
        let t = resolve_trace_id(&h);
        assert!(!t.is_empty());
        // Mint is a ULID (26 chars Crockford base32).
        assert_eq!(t.len(), 26);
    }
}
