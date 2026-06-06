//! Operator site-analytics read surface — Story 47.4 (Epic 47: Public Site
//! Analytics).
//!
//! Two **operator-scoped** read endpoints that power the dashboard `/analytics`
//! page in `apps/web`:
//!
//!   * `GET /v1/analytics/site-overview?period=7d|30d` — sessions-per-day
//!     sparkline, top-5 pages, top-5 referrer domains.
//!   * `GET /v1/analytics/funnels` — contribute funnel step counts + drop-off,
//!     verify funnel counts by method, and daily badge embeds (last 30 d).
//!
//! Both are mounted on the **operator surface** in `apps/api/src/lib.rs`
//! (`v1_operator_surface`): gated by `require_api_key` but NOT the
//! `X-Anseo-Project` guard — this is global operator state, not per-project data,
//! exactly like `/v1/plugins`. An unauthenticated request gets a `401` from the
//! auth layer before reaching these handlers (AC-6).
//!
//! Privacy: every figure is read from the *aggregate* rollup tables
//! (`site_event_rollups`, `site_page_rollups`, `site_referrer_rollups`) — never
//! from raw per-visitor rows — so the surface is privacy-safe by construction
//! (architecture A2). The single exception is the verify-by-method breakdown,
//! which reads the non-PII `method` enum from raw `site_events` within the 30-day
//! retention window (the rollup does not carry the method dimension).
//!
//! NO MCP parity: analytics is operator-internal operational data, not an
//! agent-facing prompt tool (AC-7). This is a documented parity exception (see
//! `docs/plugin-surface-boundary.md`).
//!
//! Dynamic sqlx only — no `query!` macros.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::AppState;

type ApiError = (StatusCode, Json<JsonValue>);

/// How many entries to return in the top-pages / top-referrers tables.
const TOP_N: i64 = 5;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/analytics/site-overview", get(site_overview))
        .route("/analytics/funnels", get(funnels))
}

// ─────────────────────────────────────────────────────────────────────────────
// Query params
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    /// `7d` or `30d`. Anything else clamps to `7d` (we never honor an arbitrary
    /// window — the rollups are tuned for these two).
    #[serde(default)]
    pub period: Option<String>,
}

/// Map the `period` param to a day count. Only `7d` / `30d` are supported; any
/// other value (including absent) defaults to 7 days.
fn period_days(period: &Option<String>) -> i64 {
    match period.as_deref() {
        Some("30d") => 30,
        _ => 7,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/analytics/site-overview
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct DayCount {
    date: String,
    count: i64,
}

#[derive(Debug, Serialize)]
struct PageRow {
    path: String,
    views: i64,
}

#[derive(Debug, Serialize)]
struct ReferrerRow {
    domain: String,
    visits: i64,
}

#[derive(Debug, Serialize)]
struct SiteOverview {
    period_days: i64,
    sessions_per_day: Vec<DayCount>,
    top_pages: Vec<PageRow>,
    top_referrers: Vec<ReferrerRow>,
}

fn internal_error(e: impl std::fmt::Display) -> ApiError {
    tracing::error!(event = "site_analytics.query_failed", error = %e, "analytics query failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "internal_error", "message": "failed to load analytics" })),
    )
}

async fn site_overview(
    State(state): State<AppState>,
    Query(q): Query<PeriodQuery>,
) -> Result<Json<SiteOverview>, ApiError> {
    let days = period_days(&q.period);
    let repo = state.storage.site_events();

    let sessions = repo.sessions_per_day(days).await.map_err(internal_error)?;
    let pages = repo.top_pages(days, TOP_N).await.map_err(internal_error)?;
    let referrers = repo
        .top_referrers(days, TOP_N)
        .await
        .map_err(internal_error)?;

    Ok(Json(SiteOverview {
        period_days: days,
        sessions_per_day: sessions
            .into_iter()
            .map(|d| DayCount {
                date: d.day.to_string(),
                count: d.count,
            })
            .collect(),
        top_pages: pages
            .into_iter()
            .map(|p| PageRow {
                path: p.label,
                views: p.count,
            })
            .collect(),
        top_referrers: referrers
            .into_iter()
            .map(|r| ReferrerRow {
                domain: r.label,
                visits: r.count,
            })
            .collect(),
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/analytics/funnels
// ─────────────────────────────────────────────────────────────────────────────

/// One step of a funnel: an event-type label, its count, and the drop-off rate
/// from the previous step. `drop_off_pct` is `None` for the first step and for
/// the "tracking deployed mid-funnel" anomaly where a later step has MORE events
/// than the prior one (we render `N/A` rather than a negative percentage — spec
/// note).
#[derive(Debug, Serialize)]
struct FunnelStep {
    label: String,
    count: i64,
    drop_off_pct: Option<f64>,
}

#[derive(Debug, Serialize, Default)]
struct VerifyMethod {
    method: String,
    start: i64,
    complete: i64,
    fail: i64,
    /// complete / start as a 0..=100 percentage; `None` when start == 0.
    success_rate_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Funnels {
    period_days: i64,
    contribute: Vec<FunnelStep>,
    verify: Vec<VerifyMethod>,
    badge_embeds_per_day: Vec<DayCount>,
}

/// Build a funnel with per-step drop-off. `drop_off_pct[i]` = how many of the
/// previous step's events did NOT reach step `i`, as a percentage. Returns
/// `None` for the first step and for any step that *grew* relative to the prior
/// one (anomaly: e.g. tracking deployed mid-funnel) — rendered as `N/A`.
fn build_funnel(steps: &[(&str, i64)]) -> Vec<FunnelStep> {
    let mut out = Vec::with_capacity(steps.len());
    let mut prev: Option<i64> = None;
    for (label, count) in steps {
        let drop_off_pct = match prev {
            None => None,
            Some(p) if p <= 0 => None,
            Some(p) if *count > p => None, // later step grew → N/A, never negative
            Some(p) => Some(((p - *count) as f64 / p as f64) * 100.0),
        };
        out.push(FunnelStep {
            label: (*label).to_string(),
            count: *count,
            drop_off_pct,
        });
        prev = Some(*count);
    }
    out
}

async fn funnels(
    State(state): State<AppState>,
    Query(q): Query<PeriodQuery>,
) -> Result<Json<Funnels>, ApiError> {
    let days = period_days(&q.period);
    let repo = state.storage.site_events();

    let totals = repo.event_type_totals(days).await.map_err(internal_error)?;
    let count_of = |et: &str| -> i64 {
        totals
            .iter()
            .find(|t| t.event_type == et)
            .map(|t| t.count)
            .unwrap_or(0)
    };

    // Contribute funnel. The 47.1 rollup aggregates by event_type only, so the
    // per-step breakdown (consent / api_key / submit) carried in raw
    // `contribute_step.properties.step` is not preserved — we present the funnel
    // at event-type granularity: start → step (all interim steps) → complete.
    let contribute = build_funnel(&[
        ("contribute_start", count_of("contribute_start")),
        ("contribute_step", count_of("contribute_step")),
        ("contribute_complete", count_of("contribute_complete")),
    ]);

    // Verify funnel grouped by method (dns | email). Reads non-PII `method` enum
    // from raw events within the retention window.
    let by_method = repo
        .verify_counts_by_method(days)
        .await
        .map_err(internal_error)?;
    let mut methods: std::collections::BTreeMap<String, VerifyMethod> = Default::default();
    for (event_type, method, count) in by_method {
        let m = methods
            .entry(method.clone())
            .or_insert_with(|| VerifyMethod {
                method,
                ..Default::default()
            });
        match event_type.as_str() {
            "verify_start" => m.start = count,
            "verify_complete" => m.complete = count,
            "verify_fail" => m.fail = count,
            _ => {}
        }
    }
    let verify: Vec<VerifyMethod> = methods
        .into_values()
        .map(|mut m| {
            m.success_rate_pct = if m.start > 0 {
                Some((m.complete as f64 / m.start as f64) * 100.0)
            } else {
                None
            };
            m
        })
        .collect();

    // Badge embeds — always last 30 d per spec, independent of the period param.
    let badges = repo
        .badge_embeds_per_day(30)
        .await
        .map_err(internal_error)?;

    Ok(Json(Funnels {
        period_days: days,
        contribute,
        verify,
        badge_embeds_per_day: badges
            .into_iter()
            .map(|d| DayCount {
                date: d.day.to_string(),
                count: d.count,
            })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_days_only_honors_7_and_30() {
        assert_eq!(period_days(&Some("7d".into())), 7);
        assert_eq!(period_days(&Some("30d".into())), 30);
        assert_eq!(period_days(&None), 7);
        assert_eq!(period_days(&Some("99d".into())), 7);
        assert_eq!(period_days(&Some("garbage".into())), 7);
    }

    #[test]
    fn funnel_drop_off_is_computed_between_steps() {
        let f = build_funnel(&[("start", 100), ("consent", 62), ("complete", 41)]);
        assert_eq!(f[0].drop_off_pct, None); // first step
        assert!((f[1].drop_off_pct.unwrap() - 38.0).abs() < 1e-9); // 100→62 = 38%
        assert!((f[2].drop_off_pct.unwrap() - (21.0 / 62.0 * 100.0)).abs() < 1e-9);
    }

    #[test]
    fn funnel_later_step_growth_renders_na_not_negative() {
        // Tracking deployed mid-funnel: a later step has MORE events.
        let f = build_funnel(&[("start", 10), ("step", 25), ("complete", 5)]);
        assert_eq!(
            f[1].drop_off_pct, None,
            "growth must be N/A, never negative"
        );
        // 25 → 5 is a normal 80% drop.
        assert!((f[2].drop_off_pct.unwrap() - 80.0).abs() < 1e-9);
    }

    #[test]
    fn funnel_zero_prior_step_is_na() {
        let f = build_funnel(&[("start", 0), ("complete", 0)]);
        assert_eq!(f[1].drop_off_pct, None);
    }
}
