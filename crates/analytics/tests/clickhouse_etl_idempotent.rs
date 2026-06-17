//! Story 14.1 — ETL idempotency test.
//!
//! Runs `migrate_project` against a seeded Postgres project twice; the
//! ClickHouse row counts should match between runs. The minimal ETL
//! resets the project's rows at the start of each run before re-inserting,
//! so the second run produces identical state.
//!
//! Run with:
//!
//!     CLICKHOUSE_URL=http://localhost:8123 \
//!     CLICKHOUSE_USER=anseo CLICKHOUSE_PASSWORD=... \
//!     CLICKHOUSE_DATABASE=anseo_analytics \
//!     DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo \
//!     cargo test -p anseo-analytics --features clickhouse \
//!       --test clickhouse_etl_idempotent -- --ignored

#![cfg(feature = "clickhouse")]

use anseo_analytics::metrics_store::clickhouse::ClickHouseMetricsStore;
use anseo_analytics::metrics_store::clickhouse_etl::migrate_project;
use anseo_core::ProjectId;
use anseo_storage::models::{ProjectRow, PromptRow};
use anseo_storage::repositories::{projects::ProjectRepo, prompts::PromptRepo};
use chrono::{Duration, TimeZone, Utc};
use sqlx::PgPool;
use uuid::Uuid;

async fn pg() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    PgPool::connect(&url).await.expect("connect postgres")
}

fn ch() -> ClickHouseMetricsStore {
    ClickHouseMetricsStore::from_env().expect("CLICKHOUSE_* required")
}

async fn count(ch: &ClickHouseMetricsStore, project_id: ProjectId) -> (i64, i64) {
    let bytes: [u8; 16] = project_id.into_ulid().to_bytes();
    let pid_uuid = Uuid::from_bytes(bytes).to_string();
    let vp_sql =
        format!("SELECT count() AS n FROM visibility_points WHERE project_id = '{pid_uuid}'");
    let ct_sql =
        format!("SELECT count() AS n FROM citation_totals WHERE project_id = '{pid_uuid}'");

    // Warm the public execute surface; the count is read back via run_count.
    let _ = ch.execute(&vp_sql).await;

    // Re-query via a select helper available through the public surface.
    let vp_count = run_count(ch, &vp_sql).await;
    let ct_count = run_count(ch, &ct_sql).await;
    (vp_count, ct_count)
}

async fn run_count(_ch: &ClickHouseMetricsStore, sql: &str) -> i64 {
    // ClickHouse returns count() as a UInt64 → ship as toString().
    let wrapped = sql.replace("count()", "toString(count())");
    #[derive(serde::Deserialize)]
    struct Row {
        n: String,
    }
    let url = format!(
        "{}/?database={}",
        std::env::var("CLICKHOUSE_URL")
            .unwrap()
            .trim_end_matches('/'),
        std::env::var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".into())
    );
    let body = format!("{wrapped} FORMAT JSONEachRow");
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .basic_auth(
            std::env::var("CLICKHOUSE_USER").unwrap(),
            Some(std::env::var("CLICKHOUSE_PASSWORD").unwrap()),
        )
        .body(body)
        .send()
        .await
        .expect("send count");
    let text = response.text().await.expect("body");
    let row: Row = serde_json::from_str(text.trim()).expect("parse");
    row.n.parse().expect("parse i64")
}

#[tokio::test]
#[ignore = "requires DATABASE_URL + CLICKHOUSE_*"]
async fn migrate_project_twice_yields_identical_state() {
    let pool = pg().await;
    let ch_store = ch();
    ch_store.ensure_schema().await.expect("ensure schema");
    let storage = std::sync::Arc::new(anseo_storage::Storage::from_pool(pool.clone()));
    let project_id = ProjectId::new();
    let now = Utc::now();

    ProjectRepo::new(&pool)
        .insert(&ProjectRow {
            id: project_id,
            name: format!("etl-idem-{}", project_id),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed project");
    let prompt_id = anseo_core::PromptId::new();
    PromptRepo::new(&pool)
        .insert(&PromptRow {
            id: prompt_id,
            project_id,
            name: "vector-db".to_string(),
            text: "idem fixture".into(),
            tags: Vec::new(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        })
        .await
        .expect("seed prompt");

    // Seed three successful prompt runs across two providers.
    let started = Utc.with_ymd_and_hms(2026, 5, 28, 9, 0, 0).unwrap();
    for (offset, provider) in [
        (Duration::zero(), "openai"),
        (Duration::hours(1), "openai"),
        (Duration::hours(2), "anthropic"),
    ] {
        let id = anseo_core::PromptRunId::new();
        sqlx::query(
            r#"INSERT INTO prompt_runs
               (id, prompt_id, provider, provider_model_version, started_at, finished_at,
                raw_response, request_parameters, status, created_at)
               VALUES ($1, $2, $3, 'idem-1', $4, $4, '{}'::jsonb, '{}'::jsonb, 'ok', $4)"#,
        )
        .bind(id)
        .bind(prompt_id)
        .bind(provider)
        .bind(started + offset)
        .execute(&pool)
        .await
        .expect("insert run");
        sqlx::query(
            r#"INSERT INTO citations (id, prompt_run_id, domain, frequency)
               VALUES ($1, $2, 'docs.acme.com', 1)"#,
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .execute(&pool)
        .await
        .expect("seed citation");
    }

    let first = migrate_project(&storage, &ch_store, project_id, &["vector-db"], 30, 50)
        .await
        .expect("first migrate");
    let counts_a = count(&ch_store, project_id).await;
    let second = migrate_project(&storage, &ch_store, project_id, &["vector-db"], 30, 50)
        .await
        .expect("second migrate");
    let counts_b = count(&ch_store, project_id).await;

    assert!(
        first.visibility_rows_migrated > 0,
        "first migration moved at least one visibility row"
    );
    assert_eq!(
        first.visibility_rows_migrated, second.visibility_rows_migrated,
        "visibility migration is not idempotent across runs"
    );
    assert_eq!(
        first.citation_rows_migrated, second.citation_rows_migrated,
        "citation migration is not idempotent across runs"
    );
    assert_eq!(
        counts_a, counts_b,
        "ClickHouse row counts diverged between idempotent runs"
    );

    // Cleanup.
    let _ = sqlx::query("DELETE FROM citations WHERE prompt_run_id IN (SELECT id FROM prompt_runs WHERE prompt_id = $1)")
        .bind(prompt_id).execute(&pool).await;
    let _ = sqlx::query("DELETE FROM prompt_runs WHERE prompt_id = $1")
        .bind(prompt_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM prompts WHERE id = $1")
        .bind(prompt_id)
        .execute(&pool)
        .await;
    let _ = sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(project_id)
        .execute(&pool)
        .await;
    let bytes: [u8; 16] = project_id.into_ulid().to_bytes();
    let pid_uuid = Uuid::from_bytes(bytes).to_string();
    let _ = ch_store
        .execute(&format!(
            "ALTER TABLE visibility_points DELETE WHERE project_id = '{pid_uuid}'"
        ))
        .await;
    let _ = ch_store
        .execute(&format!(
            "ALTER TABLE citation_totals DELETE WHERE project_id = '{pid_uuid}'"
        ))
        .await;
}
