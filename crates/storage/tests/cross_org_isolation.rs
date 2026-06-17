//! Story 20.5 — Cross-org read property test (GA [p4-iso-1]).
//!
//! RR-Phase4-CrossOrgRead: for every seeded tenant table, a foreign org's GUC
//! must yield zero rows. This is the "every table × foreign org → empty"
//! property test companion to rls_fail_closed.rs.
//!
//! Strategy:
//!  1. Create two orgs (alpha, beta).
//!  2. Seed one row in each covered tenant table for org_alpha (as superuser).
//!  3. SET ROLE rls_tester + app.org = org_beta GUC.
//!  4. Assert COUNT(*) = 0 for every covered table.
//!  5. Verify own-org GUC reveals the row (sanity check).
//!
//! The CI user (postgres superuser) bypasses RLS. We use SET ROLE rls_tester
//! (non-superuser, NOLOGIN) so RLS policies are actually evaluated.

use sqlx::{Executor, PgPool};

// ── Setup helpers ────────────────────────────────────────────────────────────

async fn setup_rls_tester_role(pool: &PgPool) {
    pool.execute(
        "DO $$ BEGIN \
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'rls_tester') THEN \
                CREATE ROLE rls_tester NOLOGIN; \
            END IF; \
         END $$",
    )
    .await
    .expect("create rls_tester role");

    pool.execute(
        "GRANT USAGE ON SCHEMA public TO rls_tester; \
         GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO rls_tester; \
         GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO rls_tester",
    )
    .await
    .expect("grant rls_tester permissions");
}

async fn create_org(pool: &PgPool, slug: &str) -> uuid::Uuid {
    sqlx::query_scalar("INSERT INTO organizations (slug, name) VALUES ($1, $2) RETURNING id")
        .bind(slug)
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("create org {slug}: {e}"))
}

/// Seed one row in each Phase-1 tenant table for the given org_id.
/// Runs as superuser so RLS doesn't block the inserts.
async fn seed_phase1_tables(pool: &PgPool, org_id: uuid::Uuid) {
    // projects
    let project_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO projects (id, name, competitors, variants, org_id) \
         VALUES (gen_random_uuid(), 'p', '[]'::jsonb, '{}'::text[], $1) RETURNING id",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed project");

    // prompts
    let prompt_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO prompts (id, project_id, name, text, org_id) \
         VALUES (gen_random_uuid(), $1, 'q', 'q', $2) RETURNING id",
    )
    .bind(project_id)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed prompt");

    // prompt_runs
    let prompt_run_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO prompt_runs \
         (id, prompt_id, provider, provider_model_version, started_at, \
          raw_response, request_parameters, status, org_id) \
         VALUES (gen_random_uuid(), $1, 'openai', 'gpt-4o', now(), \
                 '{}'::jsonb, '{}'::jsonb, 'ok', $2) RETURNING id",
    )
    .bind(prompt_id)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("seed prompt_run");

    // mentions
    sqlx::query(
        "INSERT INTO mentions \
         (id, prompt_run_id, entity, char_offset, rank, matched_text, org_id) \
         VALUES (gen_random_uuid(), $1, 'Acme', 0, 1, 'Acme', $2)",
    )
    .bind(prompt_run_id)
    .bind(org_id)
    .execute(pool)
    .await
    .expect("seed mention");

    // citations
    sqlx::query(
        "INSERT INTO citations \
         (id, prompt_run_id, domain, frequency, org_id) \
         VALUES (gen_random_uuid(), $1, 'example.com', 1, $2)",
    )
    .bind(prompt_run_id)
    .bind(org_id)
    .execute(pool)
    .await
    .expect("seed citation");

    // api_keys
    let suffix = &org_id.to_string()[..8];
    sqlx::query(
        "INSERT INTO api_keys \
         (id, project_id, name, sha256_hash, prefix, org_id) \
         VALUES (gen_random_uuid(), $1, 'k', $2, $3, $4)",
    )
    .bind(project_id)
    .bind(format!("hash_{suffix}"))
    .bind(format!("pre_{suffix}"))
    .bind(org_id)
    .execute(pool)
    .await
    .expect("seed api_key");

    // webhooks
    sqlx::query(
        "INSERT INTO webhooks \
         (id, project_id, name, target_url, secret_ciphertext, event_kinds, org_id) \
         VALUES (gen_random_uuid(), $1, 'wh', 'https://example.com/hook', 'enc', \
                 '[\"run.completed\"]'::jsonb, $2)",
    )
    .bind(project_id)
    .bind(org_id)
    .execute(pool)
    .await
    .expect("seed webhook");

    // schedules
    sqlx::query(
        "INSERT INTO schedules \
         (id, project_id, name, cron, prompts, providers, org_id) \
         VALUES (gen_random_uuid(), $1, 'sched', '0 * * * *', \
                 '[]'::jsonb, '[]'::jsonb, $2)",
    )
    .bind(project_id)
    .bind(org_id)
    .execute(pool)
    .await
    .expect("seed schedule");

    // alert_rules
    sqlx::query(
        "INSERT INTO alert_rules \
         (project_id, name, condition, target, org_id) \
         VALUES ($1, 'ar', 'x > 0', 'project', $2)",
    )
    .bind(project_id)
    .bind(org_id)
    .execute(pool)
    .await
    .expect("seed alert_rule");
}

// ── Assertion helper ─────────────────────────────────────────────────────────

async fn count_as_rls_tester(pool: &PgPool, table: &str, org_id: uuid::Uuid) -> i64 {
    let mut conn = pool.acquire().await.expect("acquire connection");

    conn.execute(sqlx::query("SET ROLE rls_tester"))
        .await
        .unwrap_or_else(|e| panic!("SET ROLE: {e}"));

    conn.execute(sqlx::query("SELECT set_config('app.org', $1, false)").bind(org_id.to_string()))
        .await
        .unwrap_or_else(|e| panic!("set GUC: {e}"));

    let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("COUNT(*) on {table}: {e}"));

    conn.execute(sqlx::query("RESET ROLE"))
        .await
        .unwrap_or_else(|e| panic!("RESET ROLE: {e}"));

    count
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// [p4-iso-1] Foreign org GUC → zero rows on every covered tenant table.
#[sqlx::test(migrations = "./migrations")]
async fn cross_org_read_yields_zero_for_foreign_org(pool: PgPool) {
    setup_rls_tester_role(&pool).await;

    let org_alpha = create_org(&pool, "alpha").await;
    let org_beta = create_org(&pool, "beta").await;

    seed_phase1_tables(&pool, org_alpha).await;

    let tables = [
        "projects",
        "prompts",
        "prompt_runs",
        "mentions",
        "citations",
        "api_keys",
        "webhooks",
        "schedules",
        "alert_rules",
    ];

    for table in tables {
        let count = count_as_rls_tester(&pool, table, org_beta).await;
        assert_eq!(
            count, 0,
            "[p4-iso-1] foreign org must see zero rows: table={table}"
        );
    }
}

/// Own org GUC → own rows visible (sanity / false-negative guard).
#[sqlx::test(migrations = "./migrations")]
async fn own_org_guc_yields_own_rows(pool: PgPool) {
    setup_rls_tester_role(&pool).await;

    let org_alpha = create_org(&pool, "alpha").await;
    seed_phase1_tables(&pool, org_alpha).await;

    let tables = [
        "projects",
        "prompts",
        "prompt_runs",
        "mentions",
        "citations",
        "api_keys",
        "webhooks",
        "schedules",
        "alert_rules",
    ];

    for table in tables {
        let count = count_as_rls_tester(&pool, table, org_alpha).await;
        assert_eq!(
            count, 1,
            "[p4-iso-1] own org must see own row: table={table}"
        );
    }
}

/// Two orgs seeded: each sees only its own rows (no cross-contamination).
#[sqlx::test(migrations = "./migrations")]
async fn two_orgs_each_see_only_their_own_rows(pool: PgPool) {
    setup_rls_tester_role(&pool).await;

    let org_alpha = create_org(&pool, "alpha").await;
    let org_beta = create_org(&pool, "beta").await;

    seed_phase1_tables(&pool, org_alpha).await;
    seed_phase1_tables(&pool, org_beta).await;

    for table in ["projects", "prompts", "prompt_runs"] {
        let alpha_count = count_as_rls_tester(&pool, table, org_alpha).await;
        let beta_count = count_as_rls_tester(&pool, table, org_beta).await;
        assert_eq!(
            alpha_count, 1,
            "[p4-iso-1] alpha sees only its row: table={table}"
        );
        assert_eq!(
            beta_count, 1,
            "[p4-iso-1] beta sees only its row: table={table}"
        );
    }
}
