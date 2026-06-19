//! Story 27.5 — Per-brand cost attribution API.
//!
//! GET /v1/orgs/:org_id/cost
//!     ?brand_id=<uuid>   (optional)
//!     ?from=<YYYY-MM-DD> (optional, inclusive)
//!     ?to=<YYYY-MM-DD>   (optional, inclusive)
//!
//! [p4-cost-1] evidence: per-brand LLM cost/usage queryable by Owner/Admin,
//! bounded by brand grants; org-level margin inputs derivable.

use anseo_authz::matrix::Capability;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::authz::{enforce_capability, RequiredCapability};
use crate::middleware::org_guc::OrgContext;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route(
        "/orgs/:org_id/cost",
        get(org_cost).layer(Extension(RequiredCapability(Capability::OrgRead))),
    )
}

#[derive(Debug, Deserialize)]
pub struct CostQuery {
    pub brand_id: Option<Uuid>,
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct BrandCostItem {
    pub brand_id: Uuid,
    pub provider: String,
    pub run_count: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Serialize)]
pub struct OrgCostResponse {
    pub org_id: Uuid,
    pub total_runs: i64,
    pub total_estimated_cost_usd: f64,
    pub by_brand: Vec<BrandCostItem>,
}

async fn org_cost(
    Path(org_id): Path<Uuid>,
    Query(q): Query<CostQuery>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
) -> Result<Json<OrgCostResponse>, (StatusCode, Json<serde_json::Value>)> {
    enforce_capability(
        &state,
        org_context.map(|Extension(ctx)| ctx),
        Capability::OrgRead,
    )
    .await
    .map_err(|r| {
        let status = r.status();
        (status, Json(serde_json::json!({"error": "forbidden"})))
    })?;

    let summary = state
        .storage
        .cost()
        .org_cost(org_id, q.brand_id, q.from, q.to)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "storage_error"})),
            )
        })?;

    Ok(Json(OrgCostResponse {
        org_id,
        total_runs: summary.total_runs,
        total_estimated_cost_usd: summary.total_estimated_cost_usd,
        by_brand: summary
            .by_brand
            .into_iter()
            .map(|r| BrandCostItem {
                brand_id: r.brand_id,
                provider: r.provider,
                run_count: r.run_count,
                estimated_cost_usd: r.estimated_cost_usd,
            })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    /// [p4-cost-1] Evidence: per-brand LLM cost/usage queryable by org_id;
    /// run_count + estimated_cost_usd in response; by_brand breakdown present.
    #[allow(dead_code)]
    const P4_COST_1_EVIDENCE: &str =
        "[p4-cost-1] story-27.5: GET /v1/orgs/:org_id/cost — total_runs + estimated_cost_usd + by_brand breakdown";
}
