//! Postgres-backed `MetricsStore` impl — the Phase 2 default.
//!
//! Reads the Phase 1 `prompt_runs`, `mentions`, `citations` tables. No
//! schema changes; reuses the existing `crates/analytics/src/lib.rs`
//! queries as the underlying SQL.

use async_trait::async_trait;
use opengeo_storage::Storage;
use std::sync::Arc;

use crate::metrics_store::{
    AnomalyRankSample, CitationSummaryRow, MetricsStore, MetricsStoreError, SummaryParams,
    TrendParams, VisibilityPoint,
};

pub struct PostgresMetricsStore {
    storage: Arc<Storage>,
}

impl PostgresMetricsStore {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl MetricsStore for PostgresMetricsStore {
    async fn visibility_trend(
        &self,
        project_id: opengeo_core::ProjectId,
        prompt_slug: &str,
        params: TrendParams,
    ) -> Result<Vec<VisibilityPoint>, MetricsStoreError> {
        // VisibilityPoint is the same type whether reached via the
        // legacy crate::visibility_trend or this trait — re-exported
        // in metrics_store.rs to keep one source of truth.
        let rows = crate::visibility_trend(&self.storage, project_id, prompt_slug, params.days)
            .await?;
        Ok(rows)
    }

    async fn citation_summary(
        &self,
        project_id: opengeo_core::ProjectId,
        params: SummaryParams,
    ) -> Result<Vec<CitationSummaryRow>, MetricsStoreError> {
        let rows = crate::citation_summary(&self.storage, project_id, params.limit).await?;
        Ok(rows)
    }

    async fn anomaly_samples(
        &self,
        _project_id: opengeo_core::ProjectId,
        _prompt_slug: &str,
        _days: i32,
    ) -> Result<Vec<AnomalyRankSample>, MetricsStoreError> {
        // The underlying query that joins prompt_runs + mentions per
        // (provider, observed_at) lands alongside the orchestrator
        // change that exposes per-run rank — Phase 2 Story 10.3's
        // anomaly detector currently consumes a slice fed by the
        // caller. The trait method is here so the ClickHouse impl
        // can satisfy the same contract; the Postgres path returns
        // an empty Vec for now to keep the dashboard's empty-state
        // rendering honest.
        Ok(Vec::new())
    }

    fn backend(&self) -> &'static str {
        "postgres"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn postgres_backend_string_is_stable() {
        // The parity test attributes verdict mismatches by backend
        // name; if this string changes silently, the attribution
        // breaks.
        let lazy_pool = sqlx::PgPool::connect_lazy(
            "postgres://opengeo:opengeo@127.0.0.1:1/__metrics_backend_smoke__",
        )
        .unwrap();
        let storage = Arc::new(opengeo_storage::Storage::from_pool(lazy_pool));
        let store = PostgresMetricsStore::new(storage);
        assert_eq!(store.backend(), "postgres");
    }

    #[tokio::test]
    async fn anomaly_samples_returns_empty_vec_until_query_lands() {
        // Pin the deferred behaviour so a future contributor either
        // implements it or updates this test. Either path beats
        // silently returning the wrong data.
        let lazy_pool = sqlx::PgPool::connect_lazy(
            "postgres://opengeo:opengeo@127.0.0.1:1/__anom_smoke__",
        )
        .unwrap();
        let storage = Arc::new(opengeo_storage::Storage::from_pool(lazy_pool));
        let store = PostgresMetricsStore::new(storage);
        let result = store
            .anomaly_samples(opengeo_core::ProjectId::new(), "prompt", 14)
            .await
            .unwrap();
        assert_eq!(result, Vec::new());
    }
}
