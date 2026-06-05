//! Repository for the `recommendations` table — the storage projection of
//! the Story 19.1 wire envelope (architecture-phase3-geo-recommendations.md
//! §4 / §7.1). Runtime `sqlx::query` rather than the compile-time `query!`
//! macros, matching the `api_keys` / `benchmark_consent` pattern, so the
//! offline `.sqlx/` cache stays untouched by this story.
//!
//! `insert` is dedup-aware ([rec-4]): it relies on the
//! `recommendations_active_dedup_idx` unique partial index and `ON CONFLICT
//! ... DO NOTHING`, returning `Ok(None)` when an active row with the same
//! `(project_id, kind, input_fingerprint)` already exists.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct RecommendationRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: String,
    pub severity: String,
    pub confidence_band: String,
    pub state: String,
    pub summary: String,
    pub payload: JsonValue,
    pub traceability: JsonValue,
    pub reproducibility_class: String,
    pub reproducibility_note: Option<String>,
    pub tags: Vec<String>,
    pub input_fingerprint: String,
    pub engine_version: String,
    pub plugin_source: Option<String>,
    pub generated_at: DateTime<Utc>,
}

/// Input bag for `insert`. Carries the arch §7.1 columns. `state` is the
/// DB-side lifecycle string (the wire `LifecycleState::New` maps to
/// `'generated'`); the caller owns that mapping.
#[derive(Debug, Clone)]
pub struct NewRecommendation {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: String,
    pub severity: String,
    pub confidence_band: String,
    pub state: String,
    pub summary: String,
    pub payload: JsonValue,
    pub traceability: JsonValue,
    pub reproducibility_class: String,
    pub reproducibility_note: Option<String>,
    pub tags: Vec<String>,
    pub input_fingerprint: String,
    pub engine_version: String,
    pub plugin_source: Option<String>,
}

/// SM-14 adoption metric counts (Story 19.5). `rate` is `None` when the
/// denominator is zero (no Recommendation has surfaced yet) — callers render
/// that as "n/a" rather than dividing by zero.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sm14Metric {
    pub numerator: i64,
    pub denominator: i64,
}

impl Sm14Metric {
    pub fn rate(&self) -> Option<f64> {
        if self.denominator == 0 {
            None
        } else {
            Some(self.numerator as f64 / self.denominator as f64)
        }
    }
}

/// Per-kind adoption counts for the "what works" intelligence view.
#[derive(Debug, Clone, PartialEq)]
pub struct KindAdoption {
    pub kind: String,
    pub surfaced: i64,
    pub acted: i64,
    pub dismissed: i64,
}

pub struct RecommendationsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> RecommendationsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Insert a freshly-generated Recommendation. Dedup-aware: if an active
    /// row (`state NOT IN ('dismissed','measured','stale')`) already exists
    /// for the same `(project_id, kind, input_fingerprint)`, the partial
    /// unique index fires `ON CONFLICT DO NOTHING` and this returns
    /// `Ok(None)`. Returns `Ok(Some(id))` on a genuine insert.
    pub async fn insert(&self, rec: NewRecommendation) -> Result<Option<Uuid>, Error> {
        let row = sqlx::query(
            r#"
            INSERT INTO recommendations
                (id, project_id, kind, severity, confidence_band, state,
                 summary, payload, traceability, reproducibility_class,
                 reproducibility_note, tags, input_fingerprint, engine_version,
                 plugin_source)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (project_id, kind, input_fingerprint)
                WHERE state NOT IN ('dismissed','measured','stale')
                DO NOTHING
            RETURNING id
            "#,
        )
        .bind(rec.id)
        .bind(rec.project_id)
        .bind(&rec.kind)
        .bind(&rec.severity)
        .bind(&rec.confidence_band)
        .bind(&rec.state)
        .bind(&rec.summary)
        .bind(&rec.payload)
        .bind(&rec.traceability)
        .bind(&rec.reproducibility_class)
        .bind(&rec.reproducibility_note)
        .bind(&rec.tags)
        .bind(&rec.input_fingerprint)
        .bind(&rec.engine_version)
        .bind(&rec.plugin_source)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| r.get::<Uuid, _>("id")))
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<RecommendationRow>, Error> {
        let row = sqlx::query(
            r#"
            SELECT id, project_id, kind, severity, confidence_band, state,
                   summary, payload, traceability, reproducibility_class,
                   reproducibility_note, tags, input_fingerprint,
                   engine_version, plugin_source, generated_at
            FROM recommendations
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        row.map(row_to_rec).transpose()
    }

    /// Per-kind adoption breakdown — the "what works vs what doesn't"
    /// intelligence. For each recommendation kind: how many were surfaced,
    /// acted on, and dismissed (first-party only, plugin Kinds quarantined).
    pub async fn adoption_by_kind(&self, project_id: Uuid) -> Result<Vec<KindAdoption>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT
                kind,
                count(*) FILTER (
                    WHERE state IN ('surfaced','acknowledged','acted','measured','dismissed')
                ) AS surfaced,
                count(*) FILTER (WHERE state IN ('acted','measured')) AS acted,
                count(*) FILTER (WHERE state = 'dismissed') AS dismissed
            FROM recommendations
            WHERE project_id = $1
              AND plugin_source IS NULL
            GROUP BY kind
            ORDER BY acted DESC, surfaced DESC, kind ASC
            "#,
        )
        .bind(project_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| KindAdoption {
                kind: r.get("kind"),
                surfaced: r.get("surfaced"),
                acted: r.get("acted"),
                dismissed: r.get("dismissed"),
            })
            .collect())
    }

    /// SM-14 adoption metric for `project_id` (Story 19.5, arch §6 / `[rec-7]`).
    ///
    /// `SM-14 = count(Acted ∨ Measured) / count(Surfaced ∨ later)`. Both the
    /// numerator and denominator filter on `plugin_source IS NULL` so
    /// plugin-emitted Kinds are *quarantined* out of the metric — they still
    /// surface to users via the list/MCP path, they just never move the
    /// adoption number (a plugin can't game OpenGEO's own success metric).
    ///
    /// "Surfaced ∨ later" = any row that has been shown to a user
    /// (`surfaced`/`acknowledged`/`acted`/`measured`/`dismissed`); rows still
    /// `generated` or `stale`-without-surfacing never entered the funnel.
    pub async fn sm14_metric(&self, project_id: Uuid) -> Result<Sm14Metric, Error> {
        let row = sqlx::query(
            r#"
            SELECT
                count(*) FILTER (
                    WHERE state IN ('acted','measured')
                ) AS numerator,
                count(*) FILTER (
                    WHERE state IN ('surfaced','acknowledged','acted','measured','dismissed')
                ) AS denominator
            FROM recommendations
            WHERE project_id = $1
              AND plugin_source IS NULL
            "#,
        )
        .bind(project_id)
        .fetch_one(self.pool)
        .await?;
        let numerator: i64 = row.get("numerator");
        let denominator: i64 = row.get("denominator");
        Ok(Sm14Metric {
            numerator,
            denominator,
        })
    }

    /// Active rows for `project_id`, newest first.
    pub async fn find_active_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<RecommendationRow>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, project_id, kind, severity, confidence_band, state,
                   summary, payload, traceability, reproducibility_class,
                   reproducibility_note, tags, input_fingerprint,
                   engine_version, plugin_source, generated_at
            FROM recommendations
            WHERE project_id = $1
              AND state NOT IN ('dismissed','measured','stale')
            ORDER BY generated_at DESC
            "#,
        )
        .bind(project_id)
        .fetch_all(self.pool)
        .await?;
        rows.into_iter().map(row_to_rec).collect()
    }

    /// Cursor-paginated active rows for `project_id` (Story 19.6
    /// `GET /v1/recommendations`). Keyset on `(generated_at DESC, id DESC)` so
    /// the page boundary is stable under concurrent inserts. `after` is the
    /// last `(generated_at, id)` of the previous page; `None` returns the first
    /// page. Fetches `limit` rows.
    pub async fn list_active_paginated(
        &self,
        project_id: Uuid,
        limit: i64,
        after: Option<(DateTime<Utc>, Uuid)>,
    ) -> Result<Vec<RecommendationRow>, Error> {
        let base = r#"
            SELECT id, project_id, kind, severity, confidence_band, state,
                   summary, payload, traceability, reproducibility_class,
                   reproducibility_note, tags, input_fingerprint,
                   engine_version, plugin_source, generated_at
            FROM recommendations
            WHERE project_id = $1
              AND state NOT IN ('dismissed','measured','stale')
        "#;
        let rows = match after {
            None => {
                sqlx::query(&format!(
                    "{base} ORDER BY generated_at DESC, id DESC LIMIT $2"
                ))
                .bind(project_id)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
            Some((ts, id)) => {
                sqlx::query(&format!(
                    "{base} AND (generated_at, id) < ($2, $3) \
                     ORDER BY generated_at DESC, id DESC LIMIT $4"
                ))
                .bind(project_id)
                .bind(ts)
                .bind(id)
                .bind(limit)
                .fetch_all(self.pool)
                .await?
            }
        };
        rows.into_iter().map(row_to_rec).collect()
    }

    /// Apply a lifecycle state transition, scoped to `project_id` so a row from
    /// another project can never be moved across the tenant boundary. Returns
    /// the updated row, or `Ok(None)` when no row matches (caller maps to 404).
    /// The legality of `from -> to` is enforced by the caller against
    /// `anseo_recommendations::lifecycle` before this is called.
    pub async fn update_state(
        &self,
        id: Uuid,
        project_id: Uuid,
        new_state: &str,
    ) -> Result<Option<RecommendationRow>, Error> {
        let row = sqlx::query(
            r#"
            UPDATE recommendations
            SET state = $3
            WHERE id = $1 AND project_id = $2
            RETURNING id, project_id, kind, severity, confidence_band, state,
                      summary, payload, traceability, reproducibility_class,
                      reproducibility_note, tags, input_fingerprint,
                      engine_version, plugin_source, generated_at
            "#,
        )
        .bind(id)
        .bind(project_id)
        .bind(new_state)
        .fetch_optional(self.pool)
        .await?;
        row.map(row_to_rec).transpose()
    }
}

fn row_to_rec(r: sqlx::postgres::PgRow) -> Result<RecommendationRow, Error> {
    Ok(RecommendationRow {
        id: r.try_get("id")?,
        project_id: r.try_get("project_id")?,
        kind: r.try_get("kind")?,
        severity: r.try_get("severity")?,
        confidence_band: r.try_get("confidence_band")?,
        state: r.try_get("state")?,
        summary: r.try_get("summary")?,
        payload: r.try_get("payload")?,
        traceability: r.try_get("traceability")?,
        reproducibility_class: r.try_get("reproducibility_class")?,
        reproducibility_note: r.try_get("reproducibility_note")?,
        tags: r.try_get("tags")?,
        input_fingerprint: r.try_get("input_fingerprint")?,
        engine_version: r.try_get("engine_version")?,
        plugin_source: r.try_get("plugin_source")?,
        generated_at: r.try_get("generated_at")?,
    })
}
