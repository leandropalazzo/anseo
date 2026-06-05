//! `GET /v1/benchmark/density-check` — Story 42.7: G-DENSITY gate computation.
//!
//! Returns whether all four G-DENSITY thresholds are currently met, enabling
//! named-domain mode (class-(b)) on the public benchmark dashboard.
//!
//! Thresholds (config-driven, tunable via env vars):
//!   - ANSEO_G_DENSITY_MIN_CONTRIBUTORS  (default: 20) — distinct deploying orgs
//!   - ANSEO_G_DENSITY_MIN_CATEGORIES    (default: 20) — categories above k≥5 floor
//!   - ANSEO_G_DENSITY_MIN_DOMAINS       (default: 10) — distinct ranked domains per category
//!   - ANSEO_G_DENSITY_MIN_RUNS          (default: 100) — prompt-runs per category×provider×window
//!
//! When all thresholds are met and ANSEO_G_NAME_ENABLED=true, the frontend
//! should switch from class-(a) aggregate mode to class-(b) named-domain mode.
//!
//! Unmet thresholds are logged (never silently capped) per spec AC-8.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/benchmark/density-check", get(density_check_handler))
}

/// Returns `true` when G-NAME flag is set (ANSEO_G_NAME_ENABLED=true/1/yes).
pub fn g_name_enabled() -> bool {
    matches!(
        std::env::var("ANSEO_G_NAME_ENABLED")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "true" | "1" | "yes"
    )
}

/// G-DENSITY thresholds loaded from environment. All tunable.
#[derive(Debug, Clone)]
pub struct DensityThresholds {
    pub min_contributors: i64,
    pub min_categories: i64,
    pub min_domains_per_category: i64,
    pub min_runs_per_segment: i64,
}

impl Default for DensityThresholds {
    fn default() -> Self {
        Self {
            min_contributors: parse_env("ANSEO_G_DENSITY_MIN_CONTRIBUTORS", 20),
            min_categories: parse_env("ANSEO_G_DENSITY_MIN_CATEGORIES", 20),
            min_domains_per_category: parse_env("ANSEO_G_DENSITY_MIN_DOMAINS", 10),
            min_runs_per_segment: parse_env("ANSEO_G_DENSITY_MIN_RUNS", 100),
        }
    }
}

fn parse_env(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Per-threshold pass/fail detail.
#[derive(Debug, Clone, Serialize)]
pub struct ThresholdDetail {
    pub threshold_name: &'static str,
    pub required: i64,
    pub actual: i64,
    pub passed: bool,
}

/// Full G-DENSITY gate result.
#[derive(Debug, Clone, Serialize)]
pub struct DensityCheckResponse {
    /// True only when ALL four thresholds pass AND G-NAME flag is on.
    pub named_mode_active: bool,
    /// G-NAME feature flag state.
    pub g_name_enabled: bool,
    /// Whether all density thresholds individually pass.
    pub density_thresholds_met: bool,
    /// Per-threshold breakdown for operator diagnostics.
    pub thresholds: Vec<ThresholdDetail>,
    /// ISO-8601 timestamp of this evaluation.
    pub evaluated_at: String,
    /// Methodology version for attribution (CC-NFR7).
    pub methodology_version: &'static str,
}

const METHODOLOGY_VERSION: &str = "anseo-v1.0";

async fn density_check_handler(
    State(state): State<AppState>,
) -> Result<Json<DensityCheckResponse>, (StatusCode, Json<serde_json::Value>)> {
    let thresholds = DensityThresholds::default();
    let g_name = g_name_enabled();
    let evaluated_at = chrono::Utc::now().to_rfc3339();

    let result = compute_density_check(&state, &thresholds)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "density check computation failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({
                    "error": "internal_error",
                    "message": "density check failed"
                }),
            )
        })
        .map_err(|(s, v)| (s, Json(v)))?;

    // Log any unmet thresholds (never silently cap — AC-8).
    let failed: Vec<&ThresholdDetail> = result.iter().filter(|t| !t.passed).collect();
    if !failed.is_empty() {
        let names: Vec<&str> = failed.iter().map(|t| t.threshold_name).collect();
        tracing::info!(
            event = "density_check.thresholds_unmet",
            unmet = ?names,
            "G-DENSITY gate: not all thresholds met"
        );
    }

    let all_passed = result.iter().all(|t| t.passed);

    Ok(Json(DensityCheckResponse {
        named_mode_active: g_name && all_passed,
        g_name_enabled: g_name,
        density_thresholds_met: all_passed,
        thresholds: result,
        evaluated_at,
        methodology_version: METHODOLOGY_VERSION,
    }))
}

async fn compute_density_check(
    state: &AppState,
    thresholds: &DensityThresholds,
) -> Result<Vec<ThresholdDetail>, sqlx::Error> {
    // 1. Distinct contributing deployments (operator-level project count).
    let contributors: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(DISTINCT project_id)::bigint FROM benchmark_contributions"#,
    )
    .fetch_one(state.storage.pool())
    .await
    .unwrap_or(0);

    // 2. Categories with at least one (provider × 30-day) segment above k=5.
    let categories_above_floor: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT category)::bigint
        FROM benchmark_segment_stats
        WHERE contributor_count >= 5
          AND window_days = 30
        "#,
    )
    .fetch_one(state.storage.pool())
    .await
    .unwrap_or(0);

    // 3. Minimum distinct ranked domains per category (worst-performing category).
    let min_domains_per_category: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MIN(domain_count), 0)::bigint
        FROM (
            SELECT category, COUNT(DISTINCT domain)::bigint AS domain_count
            FROM benchmark_ranked_domains
            GROUP BY category
        ) sub
        "#,
    )
    .fetch_one(state.storage.pool())
    .await
    .unwrap_or(0);

    // 4. Minimum prompt-runs per (category × provider × 30-day) segment.
    let min_runs_per_segment: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MIN(run_count), 0)::bigint
        FROM (
            SELECT category, provider, COUNT(*)::bigint AS run_count
            FROM benchmark_prompt_runs
            WHERE window_days = 30
            GROUP BY category, provider
        ) sub
        "#,
    )
    .fetch_one(state.storage.pool())
    .await
    .unwrap_or(0);

    Ok(vec![
        ThresholdDetail {
            threshold_name: "distinct_contributors",
            required: thresholds.min_contributors,
            actual: contributors,
            passed: contributors >= thresholds.min_contributors,
        },
        ThresholdDetail {
            threshold_name: "categories_above_k5_floor",
            required: thresholds.min_categories,
            actual: categories_above_floor,
            passed: categories_above_floor >= thresholds.min_categories,
        },
        ThresholdDetail {
            threshold_name: "ranked_domains_per_category",
            required: thresholds.min_domains_per_category,
            actual: min_domains_per_category,
            passed: min_domains_per_category >= thresholds.min_domains_per_category,
        },
        ThresholdDetail {
            threshold_name: "prompt_runs_per_segment",
            required: thresholds.min_runs_per_segment,
            actual: min_runs_per_segment,
            passed: min_runs_per_segment >= thresholds.min_runs_per_segment,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_default_values() {
        // Remove any test-env overrides first.
        for k in &[
            "ANSEO_G_DENSITY_MIN_CONTRIBUTORS",
            "ANSEO_G_DENSITY_MIN_CATEGORIES",
            "ANSEO_G_DENSITY_MIN_DOMAINS",
            "ANSEO_G_DENSITY_MIN_RUNS",
        ] {
            std::env::remove_var(k);
        }
        let t = DensityThresholds::default();
        assert_eq!(t.min_contributors, 20);
        assert_eq!(t.min_categories, 20);
        assert_eq!(t.min_domains_per_category, 10);
        assert_eq!(t.min_runs_per_segment, 100);
    }

    #[test]
    fn thresholds_env_override() {
        std::env::set_var("ANSEO_G_DENSITY_MIN_CONTRIBUTORS", "50");
        let t = DensityThresholds::default();
        assert_eq!(t.min_contributors, 50);
        std::env::remove_var("ANSEO_G_DENSITY_MIN_CONTRIBUTORS");
    }

    #[test]
    fn g_name_flag_off_by_default() {
        std::env::remove_var("ANSEO_G_NAME_ENABLED");
        assert!(!g_name_enabled());
    }

    #[test]
    fn g_name_flag_on() {
        std::env::set_var("ANSEO_G_NAME_ENABLED", "true");
        assert!(g_name_enabled());
        std::env::remove_var("ANSEO_G_NAME_ENABLED");
    }

    #[test]
    fn density_response_serializes() {
        let r = DensityCheckResponse {
            named_mode_active: false,
            g_name_enabled: false,
            density_thresholds_met: false,
            thresholds: vec![ThresholdDetail {
                threshold_name: "distinct_contributors",
                required: 20,
                actual: 5,
                passed: false,
            }],
            evaluated_at: "2026-06-05T00:00:00Z".into(),
            methodology_version: METHODOLOGY_VERSION,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["named_mode_active"], false);
        assert_eq!(v["thresholds"][0]["passed"], false);
        assert_eq!(
            v["thresholds"][0]["threshold_name"],
            "distinct_contributors"
        );
    }
}
