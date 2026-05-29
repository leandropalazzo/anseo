//! `GET /v1/brands` — Story 0.9 substrate for the MCP `compare_brands`
//! tool and the Extension's Add-Prompt preview dialog (epics-phase3.md
//! references both consumers).
//!
//! Returns the brand + competitors declared in `opengeo.yaml`, joined to
//! a 7-day rolling mention count and avg rank computed from
//! `mentions` × `prompt_runs`. The brand is marked `is_primary = true`;
//! competitors are `false`. Providers-with-data is the set of providers
//! that observed at least one mention of the brand in the last 7 days.
//!
//! When the API server has no `config` (CLI-less dev binds), the
//! endpoint returns 503 with a structured error rather than an empty
//! list — empty would be ambiguous with "no brand configured".
//!
//! `X-OpenGEO-Project` is accepted but not consumed; the auth middleware
//! has already resolved the project from the API key.

use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use opengeo_core::ProjectId;
use serde::Serialize;
use sqlx::Row;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route("/brands", get(list_brands))
}

#[derive(Debug, Clone, Serialize)]
pub struct BrandEntry {
    pub name: String,
    pub is_primary: bool,
    pub mention_count_7d: i64,
    /// Mean rank across the last 7 days of mentions; `None` if no
    /// mention observed.
    pub avg_rank_7d: Option<f64>,
    /// Sorted, deduped wire names.
    pub providers_with_data: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrandsResponse {
    pub items: Vec<BrandEntry>,
}

async fn list_brands(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
) -> Result<Json<BrandsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let Some(config) = state.config.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "config_unavailable",
                "message": "API server booted without a readable `opengeo.yaml`; brand list cannot be resolved.",
            })),
        ));
    };

    // Build the declaration set: primary brand first, then competitors
    // in YAML order. Variants are matched in addition to canonical names
    // for the counts query (FR-3 "configurable name variants per entity").
    let mut declared: Vec<(String, bool, Vec<String>)> = Vec::new();
    declared.push((
        config.brand.name.clone(),
        true,
        config.brand.variants.clone(),
    ));
    for comp in &config.competitors {
        declared.push((comp.name.clone(), false, comp.variants.clone()));
    }

    let mut items = Vec::with_capacity(declared.len());
    for (canonical_name, is_primary, variants) in declared {
        let stats = fetch_brand_stats(
            &state.storage,
            project_id,
            &canonical_name,
            &variants,
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, brand = %canonical_name, "brand stats fetch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "brand stats fetch failed",
                })),
            )
        })?;
        items.push(BrandEntry {
            name: canonical_name,
            is_primary,
            mention_count_7d: stats.mention_count,
            avg_rank_7d: stats.avg_rank,
            providers_with_data: stats.providers,
        });
    }

    Ok(Json(BrandsResponse { items }))
}

struct BrandStats {
    mention_count: i64,
    avg_rank: Option<f64>,
    providers: Vec<String>,
}

async fn fetch_brand_stats(
    storage: &opengeo_storage::Storage,
    project_id: ProjectId,
    canonical_name: &str,
    variants: &[String],
) -> Result<BrandStats, sqlx::Error> {
    // Case-insensitive set: canonical + variants (lowercased) for ANY()
    // membership. The `mentions.entity` column is the raw extracted
    // string — we match against the operator-declared aliases.
    let mut match_set: Vec<String> =
        std::iter::once(canonical_name.to_string())
            .chain(variants.iter().cloned())
            .map(|s| s.to_lowercase())
            .collect();
    match_set.sort();
    match_set.dedup();

    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*)::bigint                       AS mention_count,
            AVG(m.rank)::double precision          AS avg_rank
        FROM mentions m
        JOIN prompt_runs pr ON pr.id = m.prompt_run_id
        JOIN prompts     p  ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - INTERVAL '7 days'
          AND LOWER(m.entity) = ANY($2)
        "#,
    )
    .bind(project_id)
    .bind(match_set.clone())
    .fetch_one(storage.pool())
    .await?;
    let mention_count: i64 = row.try_get("mention_count")?;
    let avg_rank: Option<f64> = row.try_get("avg_rank")?;

    let prov_rows = sqlx::query(
        r#"
        SELECT DISTINCT pr.provider AS provider
        FROM mentions m
        JOIN prompt_runs pr ON pr.id = m.prompt_run_id
        JOIN prompts     p  ON p.id  = pr.prompt_id
        WHERE p.project_id = $1
          AND pr.started_at >= now() - INTERVAL '7 days'
          AND LOWER(m.entity) = ANY($2)
        ORDER BY pr.provider
        "#,
    )
    .bind(project_id)
    .bind(match_set.clone())
    .fetch_all(storage.pool())
    .await?;
    let providers: Vec<String> = prov_rows
        .into_iter()
        .map(|r| r.try_get::<String, _>("provider"))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(BrandStats {
        mention_count,
        avg_rank,
        providers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_entry_serializes_with_required_fields() {
        let e = BrandEntry {
            name: "OpenGEO".into(),
            is_primary: true,
            mention_count_7d: 42,
            avg_rank_7d: Some(2.1),
            providers_with_data: vec!["anthropic".into(), "openai".into()],
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["name"], "OpenGEO");
        assert_eq!(v["is_primary"], true);
        assert_eq!(v["mention_count_7d"], 42);
        assert_eq!(v["avg_rank_7d"], serde_json::json!(2.1));
        assert_eq!(v["providers_with_data"][0], "anthropic");
    }

    #[test]
    fn brand_entry_avg_rank_null_when_absent() {
        let e = BrandEntry {
            name: "Beta".into(),
            is_primary: false,
            mention_count_7d: 0,
            avg_rank_7d: None,
            providers_with_data: vec![],
        };
        let v = serde_json::to_value(&e).unwrap();
        assert!(v["avg_rank_7d"].is_null());
        assert_eq!(v["mention_count_7d"], 0);
        assert!(v["providers_with_data"].as_array().unwrap().is_empty());
    }

    #[test]
    fn response_wraps_items_array() {
        let r = BrandsResponse { items: vec![] };
        let v = serde_json::to_value(&r).unwrap();
        assert!(v["items"].is_array());
    }
}
