//! Phase 2 Story 10.3 / 14.1 — live-Postgres integration test for the
//! MetricsStore trait. Pins the PostgresMetricsStore backend identity
//! string and verifies the queries don't error against an empty schema.
//!
//! Gated behind the `live_db_tests` feature.

#![cfg(feature = "live_db_tests")]

use opengeo_analytics::metrics_store::postgres::PostgresMetricsStore;
use opengeo_analytics::metrics_store::{MetricsStore, SummaryParams, TrendParams};
use opengeo_core::ProjectId;
use opengeo_storage::Storage;
use std::sync::Arc;

async fn fresh_storage() -> Arc<Storage> {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let storage = Storage::connect(&url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    Arc::new(storage)
}

#[tokio::test]
#[serial_test::serial]
async fn postgres_metrics_store_backend_identity_is_postgres() {
    let storage = fresh_storage().await;
    let store = PostgresMetricsStore::new(storage);
    assert_eq!(
        store.backend(),
        "postgres",
        "MetricsStore::backend() must return the wire-stable identifier"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn postgres_metrics_store_visibility_trend_returns_ok_on_empty_project() {
    let storage = fresh_storage().await;
    let store = PostgresMetricsStore::new(storage);
    let project_id = ProjectId::new();
    let result = store
        .visibility_trend(project_id, "nonexistent-prompt", TrendParams { days: 7 })
        .await
        .expect("visibility_trend must not error on empty project");
    assert!(result.is_empty(), "no runs → empty points");
}

#[tokio::test]
#[serial_test::serial]
async fn postgres_metrics_store_citation_summary_returns_ok_on_empty_project() {
    let storage = fresh_storage().await;
    let store = PostgresMetricsStore::new(storage);
    let project_id = ProjectId::new();
    let result = store
        .citation_summary(project_id, SummaryParams { limit: 50 })
        .await
        .expect("citation_summary must not error on empty project");
    assert!(result.is_empty(), "no citations → empty summary");
}

#[tokio::test]
#[serial_test::serial]
async fn postgres_metrics_store_anomaly_samples_returns_empty_until_query_lands() {
    // Pinned deferred behavior — anomaly_samples returns an empty Vec
    // until the join-against-prompt_runs query lands. A future
    // implementation must either update this test or expose live data.
    let storage = fresh_storage().await;
    let store = PostgresMetricsStore::new(storage);
    let project_id = ProjectId::new();
    let result = store
        .anomaly_samples(project_id, "any-prompt", 14)
        .await
        .expect("anomaly_samples must not error");
    assert!(result.is_empty());
}
