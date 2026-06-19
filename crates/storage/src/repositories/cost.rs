//! Story 27.5 — Per-brand cost attribution repository.
//!
//! Queries the `brand_cost_summary` VIEW (migration 20260619130000).
//! [p4-cost-1] evidence: per-brand LLM cost queryable by Owner/Admin.

use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BrandCostRow {
    pub brand_id: Uuid,
    pub provider: String,
    pub run_count: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrgCostSummary {
    pub total_runs: i64,
    pub total_estimated_cost_usd: f64,
    pub by_brand: Vec<BrandCostRow>,
}

pub struct CostRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> CostRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Aggregate cost for an org, optionally filtered by brand and date range.
    pub async fn org_cost(
        &self,
        org_id: Uuid,
        brand_id: Option<Uuid>,
        from: Option<chrono::NaiveDate>,
        to: Option<chrono::NaiveDate>,
    ) -> Result<OrgCostSummary, sqlx::Error> {
        let rows: Vec<(Uuid, String, i64, f64)> = sqlx::query_as(
            r#"
            SELECT
                brand_id,
                provider,
                COALESCE(SUM(run_count), 0)::bigint         AS run_count,
                COALESCE(SUM(estimated_cost_usd), 0.0)::float8 AS estimated_cost_usd
            FROM brand_cost_summary
            WHERE org_id = $1
              AND ($2::uuid IS NULL OR brand_id = $2)
              AND ($3::date IS NULL OR cost_date >= $3)
              AND ($4::date IS NULL OR cost_date <= $4)
            GROUP BY brand_id, provider
            ORDER BY estimated_cost_usd DESC
            "#,
        )
        .bind(org_id)
        .bind(brand_id)
        .bind(from)
        .bind(to)
        .fetch_all(self.pool)
        .await?;

        let total_runs: i64 = rows.iter().map(|(_, _, r, _)| r).sum();
        let total_estimated_cost_usd: f64 = rows.iter().map(|(_, _, _, c)| c).sum();
        let by_brand = rows
            .into_iter()
            .map(
                |(brand_id, provider, run_count, estimated_cost_usd)| BrandCostRow {
                    brand_id,
                    provider,
                    run_count,
                    estimated_cost_usd,
                },
            )
            .collect();

        Ok(OrgCostSummary {
            total_runs,
            total_estimated_cost_usd,
            by_brand,
        })
    }
}
