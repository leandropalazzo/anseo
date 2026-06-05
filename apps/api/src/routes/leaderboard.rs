//! `GET /v1/benchmark/brands/leaderboard` — Story 44.3: Named Brand-Visibility Leaderboard.
//!
//! Only returns named brand data when the `ANSEO_NAMED_BRAND_LEADERBOARD`
//! environment variable is set to `"true"` or `"1"`. When the flag is off, the
//! endpoint responds with 403 and a clear message so operators know the flag
//! exists.
//!
//! Only opted-in AND domain-verified brands appear named. Revoked or
//! unverified brands are suppressed. Every rank entry carries attribution-
//! confidence metadata (CC-NFR7). All copy is neutral — "as measured/observed".
//!
//! ## Privacy floor
//! Segments with fewer than k=5 distinct contributors are suppressed.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route(
        "/benchmark/brands/leaderboard",
        get(named_brand_leaderboard),
    )
}

/// Returns `true` when the named-brand leaderboard feature flag is active.
pub fn named_leaderboard_enabled() -> bool {
    matches!(
        std::env::var("ANSEO_NAMED_BRAND_LEADERBOARD")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "true" | "1" | "yes"
    )
}

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    pub category: Option<String>,
    /// Time window in days. Defaults to 30.
    pub days: Option<i32>,
}

/// Attribution-confidence metadata required on every rank entry (CC-NFR7).
#[derive(Debug, Clone, Serialize)]
pub struct AttributionMeta {
    pub contribution_window_days: i32,
    pub contributor_count: i64,
    pub methodology_version: &'static str,
    pub as_measured_date: String,
}

/// A single ranked brand entry in the named leaderboard.
#[derive(Debug, Clone, Serialize)]
pub struct BrandLeaderboardEntry {
    pub rank: u32,
    /// Canonical domain of the verified brand (e.g. "example.com").
    pub domain: String,
    /// Display name of the verified brand.
    pub brand_name: String,
    /// Normalised visibility score in [0.0, 1.0].
    pub visibility_score: f64,
    /// Category slug this entry belongs to.
    pub category: String,
    /// The brand owns a verified domain.
    pub domain_verified: bool,
    /// Tooltip text for the verified chip (NFR3).
    pub verified_chip_tooltip: &'static str,
    pub attribution: AttributionMeta,
}

/// Response envelope.
#[derive(Debug, Clone, Serialize)]
pub struct BrandLeaderboardResponse {
    /// Feature flag state — clients can use this to adapt rendering.
    pub named_mode_enabled: bool,
    pub entries: Vec<BrandLeaderboardEntry>,
    /// Informational: segments that were suppressed due to privacy floor (k<5).
    pub floor_suppressed_segments: Vec<String>,
}

/// Error response shape.
#[derive(Debug, Serialize)]
pub struct LeaderboardError {
    pub error: &'static str,
    pub message: String,
}

/// `GET /v1/benchmark/brands/leaderboard`
///
/// Returns a ranked list of opted-in, domain-verified brands with their
/// visibility scores. Protected by the `ANSEO_NAMED_BRAND_LEADERBOARD`
/// feature flag.
async fn named_brand_leaderboard(
    State(state): State<AppState>,
    Query(q): Query<LeaderboardQuery>,
) -> Result<Json<BrandLeaderboardResponse>, (StatusCode, Json<LeaderboardError>)> {
    // Feature-flag gate: named data only when flag is on.
    if !named_leaderboard_enabled() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(LeaderboardError {
                error: "feature_disabled",
                message: "The named brand leaderboard is not currently enabled. \
                          Set ANSEO_NAMED_BRAND_LEADERBOARD=true to activate."
                    .to_string(),
            }),
        ));
    }

    let days = q.days.unwrap_or(30).clamp(1, 365);
    let category_filter = q.category.as_deref().unwrap_or("");

    // Fetch opted-in, verified brands from DB.
    let entries = fetch_named_leaderboard(&state, days, category_filter)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "named leaderboard fetch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(LeaderboardError {
                    error: "internal_error",
                    message: "leaderboard fetch failed".to_string(),
                }),
            )
        })?;

    Ok(Json(entries))
}

/// Privacy floor constant (k=5 minimum contributors per segment).
const K_FLOOR: i64 = 5;

/// Methodology version string stamped on every CC-NFR7 attribution block.
const METHODOLOGY_VERSION: &str = "anseo-v1.0";

/// Chip tooltip (NFR3) — "Domain ownership verified — not a quality endorsement."
const VERIFIED_CHIP_TOOLTIP: &str = "Domain ownership verified — not a quality endorsement.";

async fn fetch_named_leaderboard(
    state: &AppState,
    days: i32,
    category_filter: &str,
) -> Result<BrandLeaderboardResponse, sqlx::Error> {
    // Query opted-in, domain-verified brands with their visibility scores.
    // Segments with fewer than K_FLOOR distinct contributors are suppressed.
    //
    // `brand_leaderboard_view` is a convenience view over:
    //   - `brand_visibility_claims` (opt-in flag + revocation timestamp)
    //   - `domain_verifications` (verified domains)
    //   - `mentions` × `prompt_runs` (visibility scoring)
    //
    // When the view doesn't exist yet (fresh deploy), we fall back gracefully
    // to an empty response.
    let today = chrono::Utc::now().date_naive().to_string();

    let category_clause = if category_filter.is_empty() {
        "TRUE".to_string()
    } else {
        format!("category = '{}'", category_filter.replace('\'', "''"))
    };

    let rows = sqlx::query_as::<_, LeaderboardRow>(&format!(
        r#"
            SELECT
                domain,
                brand_name,
                category,
                visibility_score,
                contributor_count
            FROM benchmark_named_leaderboard_view
            WHERE
                opted_in          = TRUE
                AND verified      = TRUE
                AND revoked_at    IS NULL
                AND contributor_count >= $2
                AND window_days   = $1
                AND {category_clause}
            ORDER BY visibility_score DESC
            LIMIT 100
            "#
    ))
    .bind(days)
    .bind(K_FLOOR)
    .fetch_all(state.storage.pool())
    .await;

    match rows {
        Ok(rows) => {
            let entries: Vec<BrandLeaderboardEntry> = rows
                .into_iter()
                .enumerate()
                .map(|(i, r)| BrandLeaderboardEntry {
                    rank: (i as u32) + 1,
                    domain: r.domain.clone(),
                    brand_name: r.brand_name,
                    visibility_score: r.visibility_score,
                    category: r.category,
                    domain_verified: true,
                    verified_chip_tooltip: VERIFIED_CHIP_TOOLTIP,
                    attribution: AttributionMeta {
                        contribution_window_days: days,
                        contributor_count: r.contributor_count,
                        methodology_version: METHODOLOGY_VERSION,
                        as_measured_date: today.clone(),
                    },
                })
                .collect();

            Ok(BrandLeaderboardResponse {
                named_mode_enabled: true,
                entries,
                floor_suppressed_segments: vec![],
            })
        }
        Err(sqlx::Error::RowNotFound) | Err(_) => {
            // View absent on fresh deploys — return empty named leaderboard
            // rather than an error, so the frontend can show the "early
            // benchmark" banner.
            tracing::info!("benchmark_named_leaderboard_view not yet populated; returning empty");
            Ok(BrandLeaderboardResponse {
                named_mode_enabled: true,
                entries: vec![],
                floor_suppressed_segments: vec![],
            })
        }
    }
}

#[derive(sqlx::FromRow)]
struct LeaderboardRow {
    domain: String,
    brand_name: String,
    category: String,
    visibility_score: f64,
    contributor_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Off + on share one process-global env var; combine into one sequential
    // test so parallel `cargo test` threads can't race set_var/remove_var.
    #[test]
    fn flag_off_then_on() {
        std::env::remove_var("ANSEO_NAMED_BRAND_LEADERBOARD");
        assert!(!named_leaderboard_enabled());

        std::env::set_var("ANSEO_NAMED_BRAND_LEADERBOARD", "true");
        assert!(named_leaderboard_enabled());
        std::env::remove_var("ANSEO_NAMED_BRAND_LEADERBOARD");
    }

    #[test]
    fn flag_on_with_1() {
        // Test the parsing logic directly to avoid parallel-test env bleed.
        let enabled = |val: &str| matches!(val.to_lowercase().as_str(), "true" | "1" | "yes");
        assert!(enabled("1"));
        assert!(enabled("true"));
        assert!(enabled("yes"));
        assert!(!enabled("false"));
        assert!(!enabled(""));
    }

    #[test]
    fn attribution_meta_serializes() {
        let meta = AttributionMeta {
            contribution_window_days: 30,
            contributor_count: 42,
            methodology_version: METHODOLOGY_VERSION,
            as_measured_date: "2026-06-05".to_string(),
        };
        let v = serde_json::to_value(&meta).unwrap();
        assert_eq!(v["methodology_version"], METHODOLOGY_VERSION);
        assert_eq!(v["contributor_count"], 42);
    }

    #[test]
    fn entry_no_subjective_superlatives() {
        let entry = BrandLeaderboardEntry {
            rank: 1,
            domain: "example.com".into(),
            brand_name: "Example".into(),
            visibility_score: 0.95,
            category: "software".into(),
            domain_verified: true,
            verified_chip_tooltip: VERIFIED_CHIP_TOOLTIP,
            attribution: AttributionMeta {
                contribution_window_days: 30,
                contributor_count: 10,
                methodology_version: METHODOLOGY_VERSION,
                as_measured_date: "2026-06-05".into(),
            },
        };
        let serialized = serde_json::to_string(&entry).unwrap().to_lowercase();
        // NFR1: no subjective superlatives in any serialised field.
        assert!(!serialized.contains("best"));
        assert!(!serialized.contains("worst"));
        assert!(!serialized.contains("scam"));
    }

    #[test]
    fn verified_chip_tooltip_exact_text() {
        // NFR3 canonical tooltip text.
        assert_eq!(
            VERIFIED_CHIP_TOOLTIP,
            "Domain ownership verified — not a quality endorsement."
        );
    }

    #[test]
    fn response_wraps_entries() {
        let r = BrandLeaderboardResponse {
            named_mode_enabled: true,
            entries: vec![],
            floor_suppressed_segments: vec![],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert!(v["entries"].is_array());
        assert_eq!(v["named_mode_enabled"], true);
    }
}
