//! Phase 2 Story 14.1 / ARCH-26a — MetricsStore parity:
//! `PostgresMetricsStore` and `ClickHouseMetricsStore` must emit byte-
//! identical `VisibilityPoint` and `CitationSummaryRow` payloads given
//! the same fixture corpus.
//!
//! Gated #[ignore] so default cargo runs don't require both backends.
//! Run with:
//!
//!     CLICKHOUSE_URL=http://localhost:8123 \
//!     CLICKHOUSE_USER=opengeo CLICKHOUSE_PASSWORD=... \
//!     CLICKHOUSE_DATABASE=opengeo_analytics \
//!     DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo \
//!     cargo test -p opengeo-analytics --features clickhouse \
//!       --test clickhouse_parity -- --ignored

#![cfg(feature = "clickhouse")]

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use opengeo_analytics::metrics_store::{
    clickhouse::ClickHouseMetricsStore, postgres::PostgresMetricsStore, MetricsStore,
    SummaryParams, TrendParams,
};
use opengeo_core::ProjectId;
use opengeo_storage::models::{ProjectRow, PromptRow};
use opengeo_storage::repositories::{projects::ProjectRepo, prompts::PromptRepo};
use serial_test::serial;
use sqlx::PgPool;
use uuid::Uuid;

async fn pg() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    PgPool::connect(&url).await.expect("connect postgres")
}

fn ch() -> ClickHouseMetricsStore {
    ClickHouseMetricsStore::from_env().expect("CLICKHOUSE_{URL,USER,PASSWORD,DATABASE} required")
}

async fn seed_postgres_runs(
    pool: &PgPool,
    project_id: ProjectId,
    prompt_name: &str,
    runs: &[(chrono::DateTime<Utc>, &str)],
    citations: &[(usize, &str, i32, Option<&str>)],
) {
    let now = Utc::now();
    ProjectRepo::new(pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("parity-{}", project_id),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");
    let prompt_id = opengeo_core::PromptId::new();
    PromptRepo::new(pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: prompt_name.to_string(),
            text: "parity fixture".into(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed prompt");
    // Insert runs.
    let mut run_ids = Vec::with_capacity(runs.len());
    for (started_at, provider) in runs {
        let id = opengeo_core::PromptRunId::new();
        run_ids.push(id);
        sqlx::query(
            r#"INSERT INTO prompt_runs
               (id, prompt_id, provider, provider_model_version, started_at, finished_at,
                raw_response, request_parameters, status, created_at)
               VALUES ($1, $2, $3, 'parity-1', $4, $4, '{}'::jsonb, '{}'::jsonb, 'ok', $4)"#,
        )
        .bind(id)
        .bind(prompt_id)
        .bind(*provider)
        .bind(started_at)
        .execute(pool)
        .await
        .expect("insert run");
    }
    // Insert citations.
    for (run_idx, domain, frequency, source_type) in citations {
        sqlx::query(
            r#"INSERT INTO citations
               (id, prompt_run_id, domain, frequency, source_type)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(Uuid::new_v4())
        .bind(run_ids[*run_idx])
        .bind(*domain)
        .bind(*frequency)
        .bind(*source_type)
        .execute(pool)
        .await
        .expect("insert citation");
    }
}

async fn cleanup_postgres(pool: &PgPool, project_id: ProjectId) {
    let _ = sqlx::query(
        r#"DELETE FROM citations
           WHERE prompt_run_id IN (
               SELECT pr.id FROM prompt_runs pr
               JOIN prompts p ON p.id = pr.prompt_id
               WHERE p.project_id = $1
           )"#,
    )
    .bind(project_id)
    .execute(pool)
    .await;
    let _ = sqlx::query(
        r#"DELETE FROM prompt_runs WHERE prompt_id IN (SELECT id FROM prompts WHERE project_id=$1)"#,
    )
    .bind(project_id)
    .execute(pool)
    .await;
    let _ = sqlx::query("DELETE FROM prompts WHERE project_id=$1")
        .bind(project_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE id=$1")
        .bind(project_id)
        .execute(pool)
        .await;
}

async fn cleanup_clickhouse(ch: &ClickHouseMetricsStore, project_id: ProjectId) {
    let bytes: [u8; 16] = project_id.into_ulid().to_bytes();
    let pid_uuid = Uuid::from_bytes(bytes).to_string();
    let _ = ch
        .execute(&format!(
            "ALTER TABLE visibility_points DELETE WHERE project_id = '{pid_uuid}'"
        ))
        .await;
    let _ = ch
        .execute(&format!(
            "ALTER TABLE citation_totals DELETE WHERE project_id = '{pid_uuid}'"
        ))
        .await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL + CLICKHOUSE_*"]
#[serial]
async fn citation_summary_parity_across_backends() {
    let pool = pg().await;
    let ch_store = ch();
    ch_store.ensure_schema().await.expect("ensure ch schema");
    let project_id = ProjectId::new();
    let prompt = "vector-db";

    // Two runs: openai cited docs.acme.com twice + arxiv.org once;
    // anthropic cited docs.acme.com once.
    let t = Utc.with_ymd_and_hms(2026, 5, 25, 9, 0, 0).unwrap();
    seed_postgres_runs(
        &pool,
        project_id,
        prompt,
        &[(t, "openai"), (t + Duration::hours(1), "anthropic")],
        &[
            (0, "docs.acme.com", 2, Some("docs")),
            (0, "arxiv.org", 1, Some("paper")),
            (1, "docs.acme.com", 1, Some("docs")),
        ],
    )
    .await;

    // Mirror the citation totals into ClickHouse exactly as the ETL would.
    ch_store
        .seed_citation_totals(&[
            (project_id, "docs.acme.com", 3, Some("docs")),
            (project_id, "arxiv.org", 1, Some("paper")),
        ])
        .await
        .expect("seed ch citation_totals");

    let pg_store =
        PostgresMetricsStore::new(Arc::new(opengeo_storage::Storage::from_pool(pool.clone())));
    let from_pg = pg_store
        .citation_summary(project_id, SummaryParams { limit: 50 })
        .await
        .expect("pg citation_summary");
    let from_ch = ch_store
        .citation_summary(project_id, SummaryParams { limit: 50 })
        .await
        .expect("ch citation_summary");

    assert_eq!(
        from_pg.len(),
        from_ch.len(),
        "row counts disagree:\n  postgres={from_pg:?}\n  clickhouse={from_ch:?}"
    );
    for (a, b) in from_pg.iter().zip(from_ch.iter()) {
        assert_eq!(a.domain, b.domain, "domain order disagrees");
        assert_eq!(
            a.frequency, b.frequency,
            "frequency disagrees for {}",
            a.domain
        );
        assert_eq!(
            a.source_type, b.source_type,
            "source_type disagrees for {}",
            a.domain
        );
    }

    cleanup_postgres(&pool, project_id).await;
    cleanup_clickhouse(&ch_store, project_id).await;
}

#[tokio::test]
#[ignore = "requires DATABASE_URL + CLICKHOUSE_*"]
#[serial]
async fn visibility_trend_parity_across_backends() {
    let pool = pg().await;
    let ch_store = ch();
    ch_store.ensure_schema().await.expect("ensure ch schema");
    let project_id = ProjectId::new();
    let prompt = "vector-db";

    // Three days of runs; openai succeeded every day, anthropic only
    // succeeded on day two. The visibility_trend SQL emits one row per
    // (date, provider) bucket — for the parity test we directly seed
    // the pre-aggregated bucket payload into ClickHouse to mirror the
    // ETL output.
    let day0 = Utc.with_ymd_and_hms(2026, 5, 27, 9, 0, 0).unwrap();
    let day1 = Utc.with_ymd_and_hms(2026, 5, 28, 9, 0, 0).unwrap();
    let day2 = Utc.with_ymd_and_hms(2026, 5, 29, 9, 0, 0).unwrap();
    seed_postgres_runs(
        &pool,
        project_id,
        prompt,
        &[
            (day0, "openai"),
            (day1, "openai"),
            (day1, "anthropic"),
            (day2, "openai"),
        ],
        &[],
    )
    .await;

    // The Postgres `visibility_trend` query truncates to day buckets
    // and emits `presence_rate = 1.0` for each (Phase 1 placeholder).
    // Seed ClickHouse with the same buckets.
    let day0_bucket = day0.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let day1_bucket = day1.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let day2_bucket = day2.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    ch_store
        .seed_visibility_points(&[
            (project_id, prompt, "anthropic", day1_bucket, None, 1.0),
            (project_id, prompt, "openai", day0_bucket, None, 1.0),
            (project_id, prompt, "openai", day1_bucket, None, 1.0),
            (project_id, prompt, "openai", day2_bucket, None, 1.0),
        ])
        .await
        .expect("seed ch visibility_points");

    let pg_store =
        PostgresMetricsStore::new(Arc::new(opengeo_storage::Storage::from_pool(pool.clone())));
    let from_pg = pg_store
        .visibility_trend(project_id, prompt, TrendParams { days: 30 })
        .await
        .expect("pg visibility_trend");
    let from_ch = ch_store
        .visibility_trend(project_id, prompt, TrendParams { days: 30 })
        .await
        .expect("ch visibility_trend");

    assert_eq!(
        from_pg.len(),
        from_ch.len(),
        "row counts disagree:\n  postgres={from_pg:?}\n  clickhouse={from_ch:?}"
    );
    // Postgres `visibility_trend` orders by bucket_start only; the
    // tie-break on provider isn't guaranteed across backends, so we
    // sort both before comparing.
    let mut from_pg = from_pg;
    let mut from_ch = from_ch;
    let sort_key = |p: &opengeo_analytics::VisibilityPoint| (p.bucket_start, p.provider.clone());
    from_pg.sort_by_key(sort_key);
    from_ch.sort_by_key(sort_key);
    for (a, b) in from_pg.iter().zip(from_ch.iter()) {
        assert_eq!(a.bucket_start, b.bucket_start, "bucket_start disagrees");
        assert_eq!(a.provider, b.provider, "provider disagrees");
        assert_eq!(a.avg_rank, b.avg_rank, "avg_rank disagrees");
        assert_eq!(a.presence_rate, b.presence_rate, "presence_rate disagrees");
    }

    cleanup_postgres(&pool, project_id).await;
    cleanup_clickhouse(&ch_store, project_id).await;
}

#[tokio::test]
#[ignore = "requires CLICKHOUSE_*"]
async fn backend_strings_disambiguate() {
    let pg_pool =
        sqlx::PgPool::connect_lazy("postgres://opengeo:opengeo@127.0.0.1:1/__parity_smoke__")
            .unwrap();
    let pg_store =
        PostgresMetricsStore::new(Arc::new(opengeo_storage::Storage::from_pool(pg_pool)));
    let ch_store = ch();
    assert_eq!(pg_store.backend(), "postgres");
    assert_eq!(ch_store.backend(), "clickhouse");
    assert_ne!(pg_store.backend(), ch_store.backend());
}
