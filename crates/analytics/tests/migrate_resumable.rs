//! Story 0.1 — resumable ETL: interrupt+resume == single-pass.
//!
//! Seeds a project, runs the resumable Postgres→ClickHouse ETL once as a
//! single pass, snapshots the ClickHouse-backed query results, then re-runs
//! the same project as an *interrupted* migration (SIGTERM simulated after one
//! batch via `stop_after_batches`) followed by a resume. The post-resume
//! ClickHouse state must be byte-for-byte identical to the single-pass state,
//! and the `analytics_migration_state` checkpoint must reflect interrupted →
//! finished transitions.
//!
//! Run with:
//!
//!     CLICKHOUSE_URL=http://localhost:8123 \
//!     CLICKHOUSE_USER=opengeo CLICKHOUSE_PASSWORD=... \
//!     CLICKHOUSE_DATABASE=anseo_analytics \
//!     DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo \
//!     cargo test -p opengeo-analytics --features "clickhouse live_db_tests" \
//!       --test migrate_resumable -- --ignored

#![cfg(all(feature = "clickhouse", feature = "live_db_tests"))]

use std::sync::Arc;

use anseo_analytics::metrics_store::clickhouse::ClickHouseMetricsStore;
use anseo_analytics::metrics_store::clickhouse_etl::{migrate_project_resumable, ResumableConfig};
use anseo_analytics::metrics_store::{MetricsStore, SummaryParams, TrendParams};
use anseo_core::ProjectId;
use anseo_storage::models::{ProjectRow, PromptRow};
use anseo_storage::repositories::{projects::ProjectRepo, prompts::PromptRepo};
use anseo_storage::Storage;
use chrono::{Duration, TimeZone, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

const PROMPT: &str = "vector-db";

async fn pg() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    PgPool::connect(&url).await.expect("connect postgres")
}

fn ch() -> ClickHouseMetricsStore {
    ClickHouseMetricsStore::from_env().expect("CLICKHOUSE_* required")
}

/// Canonical, order-stable repr of this project's ClickHouse-backed analytics,
/// used as the byte-for-byte equality oracle.
async fn snapshot(ch: &ClickHouseMetricsStore, project_id: ProjectId) -> Vec<String> {
    let mut out = Vec::new();
    let mut vis = ch
        .visibility_trend(project_id, PROMPT, TrendParams { days: 60 })
        .await
        .expect("ch visibility_trend");
    vis.sort_by(|a, b| {
        (a.bucket_start, &a.provider)
            .partial_cmp(&(b.bucket_start, &b.provider))
            .unwrap()
    });
    for p in &vis {
        out.push(format!(
            "VIS|{}|{}|{:?}|{}",
            p.bucket_start.to_rfc3339(),
            p.provider,
            p.avg_rank,
            p.presence_rate
        ));
    }
    let cit = ch
        .citation_summary(project_id, SummaryParams { limit: 100 })
        .await
        .expect("ch citation_summary");
    for r in &cit {
        out.push(format!(
            "CIT|{}|{}|{:?}",
            r.domain, r.frequency, r.source_type
        ));
    }
    out
}

async fn checkpoint(pool: &PgPool, project_id: ProjectId) -> (i64, bool) {
    let row = sqlx::query(
        "SELECT last_completed_batch_id, finished_at IS NOT NULL AS done \
         FROM analytics_migration_state WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await
    .expect("checkpoint row");
    (
        row.get::<i64, _>("last_completed_batch_id"),
        row.get::<bool, _>("done"),
    )
}

async fn seed(pool: &PgPool, project_id: ProjectId) {
    let now = Utc::now();
    ProjectRepo::new(pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("resumable-{project_id}"),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");
    let prompt_id = anseo_core::PromptId::new();
    PromptRepo::new(pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: PROMPT.to_string(),
            text: "resumable fixture".into(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed prompt");

    // Three days × providers → several visibility buckets, plus citations,
    // so batch_size=1 forces multiple batches with a real mid-point interrupt.
    let day0 = Utc.with_ymd_and_hms(2026, 5, 26, 9, 0, 0).unwrap();
    for (offset_days, provider) in [
        (0i64, "openai"),
        (1, "openai"),
        (1, "anthropic"),
        (2, "openai"),
    ] {
        let id = anseo_core::PromptRunId::new();
        let started = day0 + Duration::days(offset_days);
        sqlx::query(
            r#"INSERT INTO prompt_runs
               (id, prompt_id, provider, provider_model_version, started_at, finished_at,
                raw_response, request_parameters, status, created_at)
               VALUES ($1, $2, $3, 'resume-1', $4, $4, '{}'::jsonb, '{}'::jsonb, 'ok', $4)"#,
        )
        .bind(id)
        .bind(prompt_id)
        .bind(provider)
        .bind(started)
        .execute(pool)
        .await
        .expect("insert run");
        for (domain, freq) in [("docs.acme.com", 2), ("arxiv.org", 1)] {
            sqlx::query(
                r#"INSERT INTO citations (id, prompt_run_id, domain, frequency)
                   VALUES ($1, $2, $3, $4)"#,
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(domain)
            .bind(freq)
            .execute(pool)
            .await
            .expect("seed citation");
        }
    }
}

async fn cleanup(pool: &PgPool, ch: &ClickHouseMetricsStore, project_id: ProjectId) {
    let _ = sqlx::query("DELETE FROM citations WHERE prompt_run_id IN (SELECT pr.id FROM prompt_runs pr JOIN prompts p ON p.id = pr.prompt_id WHERE p.project_id = $1)")
        .bind(project_id).execute(pool).await;
    let _ = sqlx::query(
        "DELETE FROM prompt_runs WHERE prompt_id IN (SELECT id FROM prompts WHERE project_id = $1)",
    )
    .bind(project_id)
    .execute(pool)
    .await;
    let _ = sqlx::query("DELETE FROM prompts WHERE project_id = $1")
        .bind(project_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM analytics_migration_state WHERE project_id = $1")
        .bind(project_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(project_id)
        .execute(pool)
        .await;
    let pid = Uuid::from_bytes(project_id.into_ulid().to_bytes()).to_string();
    let _ = ch
        .execute(&format!(
            "ALTER TABLE visibility_points DELETE WHERE project_id = '{pid}'"
        ))
        .await;
    let _ = ch
        .execute(&format!(
            "ALTER TABLE citation_totals DELETE WHERE project_id = '{pid}'"
        ))
        .await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL + CLICKHOUSE_*"]
async fn interrupt_then_resume_equals_single_pass() {
    let pool = pg().await;
    let ch_store = ch();
    ch_store.ensure_schema().await.expect("ensure schema");
    let storage = Arc::new(Storage::from_pool(pool.clone()));
    storage.migrate().await.expect("apply migrations");
    let project_id = ProjectId::new();
    cleanup(&pool, &ch_store, project_id).await;
    seed(&pool, project_id).await;

    // 1) Single pass (batch_size=1 to maximize batch count).
    let single = migrate_project_resumable(
        &storage,
        &ch_store,
        project_id,
        &[PROMPT],
        60,
        100,
        &ResumableConfig {
            batch_size: 1,
            stop_after_batches: None,
        },
    )
    .await
    .expect("single pass");
    assert!(single.finished, "single pass should finish");
    assert!(
        single.total_batches >= 2,
        "fixture should produce >= 2 batches"
    );
    let single_snapshot = snapshot(&ch_store, project_id).await;
    let (cp_batches, cp_done) = checkpoint(&pool, project_id).await;
    assert!(cp_done, "checkpoint marked finished after single pass");
    assert_eq!(
        cp_batches as usize, single.total_batches,
        "checkpoint == total batches"
    );

    // 2) Interrupted run: a fresh start (prior run finished) that stops after
    //    one batch, leaving the checkpoint resumable.
    let interrupted = migrate_project_resumable(
        &storage,
        &ch_store,
        project_id,
        &[PROMPT],
        60,
        100,
        &ResumableConfig {
            batch_size: 1,
            stop_after_batches: Some(1),
        },
    )
    .await
    .expect("interrupted run");
    assert!(!interrupted.finished, "interrupted run must not finish");
    assert_eq!(
        interrupted.batches_this_run, 1,
        "interrupt stopped after one batch"
    );
    let (mid_batches, mid_done) = checkpoint(&pool, project_id).await;
    assert!(!mid_done, "checkpoint must be unfinished mid-interrupt");
    assert_eq!(mid_batches, 1, "checkpoint advanced exactly one batch");

    // 3) Resume to completion.
    let resumed = migrate_project_resumable(
        &storage,
        &ch_store,
        project_id,
        &[PROMPT],
        60,
        100,
        &ResumableConfig {
            batch_size: 1,
            stop_after_batches: None,
        },
    )
    .await
    .expect("resume");
    assert!(resumed.finished, "resume should finish");
    assert!(
        resumed.batches_this_run >= 1,
        "resume committed remaining batches"
    );
    assert_eq!(
        resumed.batches_completed, single.total_batches,
        "resume reaches the same total batch count"
    );

    // 4) Byte-for-byte: post-resume state == single-pass state.
    let resumed_snapshot = snapshot(&ch_store, project_id).await;
    assert_eq!(
        single_snapshot, resumed_snapshot,
        "interrupt+resume diverged from single-pass ClickHouse state"
    );

    cleanup(&pool, &ch_store, project_id).await;
}
