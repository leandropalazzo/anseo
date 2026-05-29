//! Story 0.12 — Repository stub for the `recommendations` table (Epic 17
//! GEO Recommendations substrate). Mirrors the pattern used by
//! [`crate::repositories::api_keys`] / `benchmark_consent`: runtime
//! `sqlx::query` rather than the compile-time `query!` macros, because
//! Story 0.12 lands the migration without a `.sqlx/` offline cache
//! entry. Once a follow-up `cargo sqlx prepare --workspace` runs against
//! a live DB, callers can migrate to `query!` if desired.
//!
//! Surface intentionally minimal — `insert`, `find_by_id`,
//! `find_by_brand`, `update_lifecycle`. Business logic (recommender
//! producers, lifecycle transitions, outcome window jobs) lands in
//! Epic 17 stories that consume this repo.
//!
//! Methods carry `#[allow(dead_code)]` for now because no caller exists
//! yet; the substrate ships ahead of the consumers to unblock parallel
//! Epic 17 + 19 stories. Remove the allows once consumers wire up.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct RecommendationRow {
    pub id: Uuid,
    pub kind: String,
    pub prompt: Option<String>,
    pub provider: Option<String>,
    pub brand: String,
    pub severity: String,
    pub lane: String,
    pub non_deterministic_pipeline: bool,
    pub plugin_source: Option<String>,
    pub generated_at: DateTime<Utc>,
    pub lifecycle_state: String,
    pub acted_at: Option<DateTime<Utc>>,
    pub evidence_url: Option<String>,
    pub outcome_visibility_delta: Option<f32>,
    pub outcome_window_days: Option<i32>,
    pub traceability: JsonValue,
}

/// Input bag for `insert`. Kept as a struct rather than a long argument
/// list because the `recommendations` row has 13 caller-supplied fields
/// at insert time and positional args would be a footgun.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NewRecommendation<'a> {
    pub kind: &'a str,
    pub prompt: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub brand: &'a str,
    pub severity: &'a str,
    pub lane: &'a str,
    pub non_deterministic_pipeline: bool,
    pub plugin_source: Option<&'a str>,
    pub evidence_url: Option<&'a str>,
    pub traceability: JsonValue,
}

pub struct RecommendationsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RecommendationsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Insert a freshly-generated recommendation in `lifecycle_state =
    /// 'generated'`. Caller is responsible for moving it to `surfaced`
    /// via `update_lifecycle` when the UI displays it.
    #[allow(dead_code)]
    pub async fn insert(&self, rec: NewRecommendation<'_>) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"
            INSERT INTO recommendations
                (id, kind, prompt, provider, brand, severity, lane,
                 non_deterministic_pipeline, plugin_source, evidence_url,
                 traceability)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(id)
        .bind(rec.kind)
        .bind(rec.prompt)
        .bind(rec.provider)
        .bind(rec.brand)
        .bind(rec.severity)
        .bind(rec.lane)
        .bind(rec.non_deterministic_pipeline)
        .bind(rec.plugin_source)
        .bind(rec.evidence_url)
        .bind(rec.traceability)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    #[allow(dead_code)]
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<RecommendationRow>, Error> {
        let row = sqlx::query(
            r#"
            SELECT id, kind, prompt, provider, brand, severity, lane,
                   non_deterministic_pipeline, plugin_source, generated_at,
                   lifecycle_state, acted_at, evidence_url,
                   outcome_visibility_delta, outcome_window_days, traceability
            FROM recommendations
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        row.map(row_to_rec).transpose()
    }

    /// All recommendations for `brand`, newest first. Lifecycle filter
    /// is the caller's job — surfaces typically want `lifecycle_state IN
    /// ('generated','surfaced')` but the audit view wants everything.
    #[allow(dead_code)]
    pub async fn find_by_brand(&self, brand: &str) -> Result<Vec<RecommendationRow>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, kind, prompt, provider, brand, severity, lane,
                   non_deterministic_pipeline, plugin_source, generated_at,
                   lifecycle_state, acted_at, evidence_url,
                   outcome_visibility_delta, outcome_window_days, traceability
            FROM recommendations
            WHERE brand = $1
            ORDER BY generated_at DESC
            "#,
        )
        .bind(brand)
        .fetch_all(self.pool)
        .await?;
        rows.into_iter().map(row_to_rec).collect()
    }

    /// Move a recommendation to a new lifecycle state. When the new
    /// state is `acted_on`, also stamps `acted_at = now()`. The outcome
    /// columns are NOT touched here — they're populated by the window
    /// job when transitioning `acted_on → measured`.
    #[allow(dead_code)]
    pub async fn update_lifecycle(
        &self,
        id: Uuid,
        new_state: &str,
    ) -> Result<bool, Error> {
        let result = sqlx::query(
            r#"
            UPDATE recommendations
            SET lifecycle_state = $2,
                acted_at = CASE
                    WHEN $2 = 'acted_on' AND acted_at IS NULL THEN now()
                    ELSE acted_at
                END
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(new_state)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_rec(r: sqlx::postgres::PgRow) -> Result<RecommendationRow, Error> {
    Ok(RecommendationRow {
        id: r.try_get("id")?,
        kind: r.try_get("kind")?,
        prompt: r.try_get("prompt")?,
        provider: r.try_get("provider")?,
        brand: r.try_get("brand")?,
        severity: r.try_get("severity")?,
        lane: r.try_get("lane")?,
        non_deterministic_pipeline: r.try_get("non_deterministic_pipeline")?,
        plugin_source: r.try_get("plugin_source")?,
        generated_at: r.try_get("generated_at")?,
        lifecycle_state: r.try_get("lifecycle_state")?,
        acted_at: r.try_get("acted_at")?,
        evidence_url: r.try_get("evidence_url")?,
        outcome_visibility_delta: r.try_get("outcome_visibility_delta")?,
        outcome_window_days: r.try_get("outcome_window_days")?,
        traceability: r.try_get("traceability")?,
    })
}
