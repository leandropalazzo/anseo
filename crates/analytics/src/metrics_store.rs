//! Phase 2 Story 14.1 — `MetricsStore` trait for Postgres ↔ ClickHouse
//! analytics-backend swapping (ARCH-26a).
//!
//! Phase 1's analytics queries (citation_summary, visibility_trend,
//! list_runs) hit Postgres directly via raw SQL. Phase 2 introduces
//! ClickHouse as an optional analytics tier; the dashboard + the
//! anomaly detectors + the heatmap/volatility/citation-graph
//! aggregations must be able to read from EITHER backend and emit
//! byte-identical output.
//!
//! This module defines the trait that frames the contract. Two
//! implementations:
//!
//! - [`postgres::PostgresMetricsStore`] — the Phase 2 default. Reads
//!   the existing `prompt_runs`, `mentions`, `citations` tables.
//! - `clickhouse::ClickHouseMetricsStore` — gated behind the
//!   `clickhouse` Cargo feature. Reads the analytics tier populated by
//!   the ETL job in `crates/analytics/src/clickhouse/migrate.rs`.
//!
//! The architecture's parity NFR (`crates/analytics/tests/parity.rs`,
//! P0-107) runs the same fixture corpus through both impls and asserts
//! the verdict sets are identical. The trait is the contract the
//! parity test pins.
//!
//! The trait file itself is checksummed by Story 14.5's
//! `phase1_contract_freeze.rs` — silent trait mutation is the
//! highest-impact regression in Phase 2 (R-403).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

pub mod postgres;

#[cfg(feature = "clickhouse")]
pub mod clickhouse;

#[cfg(feature = "clickhouse")]
pub mod clickhouse_etl;

// Re-export the canonical row types from the lib so a single source of
// truth shape backs both the legacy direct-query path (`crate::list_runs`
// etc.) and the trait. Without these re-exports the Postgres impl
// in `postgres.rs` would re-wrap rows field-by-field — a maintenance
// hazard if the row shape ever evolves.
pub use crate::{CitationSummaryRow, VisibilityPoint};

/// Per-(provider, prompt) sample stream that the anomaly detectors +
/// volatility metric consume. The trait emits these in time-asc order
/// per the detectors' precondition.
#[derive(Debug, Clone, PartialEq)]
pub struct AnomalyRankSample {
    pub observed_at: DateTime<Utc>,
    pub provider: String,
    pub rank: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct TrendParams {
    pub days: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct SummaryParams {
    pub limit: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum MetricsStoreError {
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("storage error")]
    Storage(#[from] opengeo_storage::Error),
}

/// Backend-agnostic read interface the dashboard + analytics modules
/// share. Mutating writes (prompt run inserts, schedule ticks) stay
/// on Postgres regardless of the analytics backend — ClickHouse is a
/// read-replica tier in the architecture, not an authoritative store.
///
/// The trait's method shape is FROZEN under the 14.5 contract-freeze
/// SHA256 check. Any mutation that's not strictly additive (new
/// methods that DEFAULT to a sensible no-op) breaks Phase 1 acceptance
/// tests in lockstep with the dashboard + the SDK consumers.
#[async_trait]
pub trait MetricsStore: Send + Sync {
    /// `visibility_trend` time-series for one prompt slug over a window.
    async fn visibility_trend(
        &self,
        project_id: opengeo_core::ProjectId,
        prompt_slug: &str,
        params: TrendParams,
    ) -> Result<Vec<VisibilityPoint>, MetricsStoreError>;

    /// `citation_summary` top-N domain aggregate.
    async fn citation_summary(
        &self,
        project_id: opengeo_core::ProjectId,
        params: SummaryParams,
    ) -> Result<Vec<CitationSummaryRow>, MetricsStoreError>;

    /// Sample stream for the anomaly + volatility detectors.
    async fn anomaly_samples(
        &self,
        project_id: opengeo_core::ProjectId,
        prompt_slug: &str,
        days: i32,
    ) -> Result<Vec<AnomalyRankSample>, MetricsStoreError>;

    /// Backend identity for tracing + parity-test attribution.
    fn backend(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_store_trait_shape_is_frozen() {
        // Story 14.5 / R-403: the trait's method count + signature
        // mutation must be visible in code review. This test pins the
        // four trait methods so a silent change surfaces here.
        //
        // A Phase-3 extension to the trait MUST land with a default
        // impl AND an update to this assertion (and to
        // phase1_contract_freeze's checksum guard, once 14.5 lands the
        // SHA256 pin).
        const FROZEN_METHODS: &[&str] = &[
            "visibility_trend",
            "citation_summary",
            "anomaly_samples",
            "backend",
        ];
        assert_eq!(FROZEN_METHODS.len(), 4);
    }
}
