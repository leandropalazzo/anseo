//! Story 20.3 — RLS fail-closed test (RR-Phase4-RlsFailClosed, GA [p4-iso-2]).
//!
//! The core invariant: `current_setting('app.org', true)` returns NULL when the
//! GUC is unset → NULL::uuid = org_id is FALSE → zero rows on every tenant table.
//!
//! Tests:
//!  1. Unset GUC → zero rows on `projects` (the anchor test).
//!  2. Correct org GUC → own row visible.
//!  3. Foreign org GUC → foreign row not visible.
//!  4. RLS policy exists on all tenant tables.
//!
//! RLS bypass note: the CI/test DB user is a PostgreSQL superuser, which bypasses
//! RLS even with FORCE ROW LEVEL SECURITY. Tests 1–3 create a non-privileged
//! `rls_tester` role, grant it table access, then SET ROLE before the assertion
//! queries so that RLS policies are actually enforced.

use sqlx::{Executor, PgPool};

// ---------------------------------------------------------------------------
// Setup: create an unprivileged role that is subject to RLS.
// The superuser test connection grants the role access but it has no LOGIN
// so it can't connect independently — SET ROLE is used to impersonate it.
// ---------------------------------------------------------------------------
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

// ---------------------------------------------------------------------------
// Helper: insert a project row as the superuser (bypasses RLS) with explicit org.
// ---------------------------------------------------------------------------
async fn insert_project_as_superuser(pool: &PgPool, org_id: uuid::Uuid, name: &str) -> uuid::Uuid {
    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (id, name, competitors, variants, org_id) \
         VALUES (gen_random_uuid(), $1, '[]'::jsonb, '{}'::text[], $2) RETURNING id",
    )
    .bind(name)
    .bind(org_id)
    .fetch_one(pool)
    .await
    .expect("insert project as superuser")
}

// ---------------------------------------------------------------------------
// 1. Unset GUC → zero rows on projects (asserted as non-superuser rls_tester)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn unset_guc_yields_zero_rows(pool: PgPool) {
    setup_rls_tester_role(&pool).await;

    let default_org: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("default org");

    insert_project_as_superuser(&pool, default_org, "rls-test-project").await;

    // Switch to the restricted role so RLS is enforced.
    let mut conn = pool.acquire().await.expect("acquire connection");
    conn.execute(sqlx::query("SET ROLE rls_tester"))
        .await
        .expect("set role");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&mut *conn)
        .await
        .expect("count projects without GUC");

    conn.execute(sqlx::query("RESET ROLE"))
        .await
        .expect("reset role");

    assert_eq!(
        count, 0,
        "Unset app.org GUC must yield zero rows (fail-closed, RR-Phase4-RlsFailClosed)"
    );
}

// ---------------------------------------------------------------------------
// 2. Correct org GUC → own row visible (asserted as rls_tester)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn correct_org_guc_yields_own_rows(pool: PgPool) {
    setup_rls_tester_role(&pool).await;

    let default_org: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("default org");

    insert_project_as_superuser(&pool, default_org, "visible-project").await;

    let mut conn = pool.acquire().await.expect("acquire connection");
    conn.execute(sqlx::query("SET ROLE rls_tester"))
        .await
        .expect("set role");
    conn.execute(sqlx::query("SET app.org = $1::text").bind(default_org.to_string()))
        .await
        .expect("set GUC");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&mut *conn)
        .await
        .expect("count with correct GUC");

    conn.execute(sqlx::query("RESET ROLE"))
        .await
        .expect("reset role");

    assert_eq!(count, 1, "Correct org GUC must reveal own rows");
}

// ---------------------------------------------------------------------------
// 3. Foreign org GUC → foreign row not visible (asserted as rls_tester)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn foreign_org_guc_yields_zero_rows(pool: PgPool) {
    setup_rls_tester_role(&pool).await;

    let org_a: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO organizations (slug, name) VALUES ('org-a', 'Org A') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert org A");

    let org_b: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO organizations (slug, name) VALUES ('org-b', 'Org B') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert org B");

    insert_project_as_superuser(&pool, org_a, "org-a-project").await;

    let mut conn = pool.acquire().await.expect("acquire connection");
    conn.execute(sqlx::query("SET ROLE rls_tester"))
        .await
        .expect("set role");
    conn.execute(sqlx::query("SET app.org = $1::text").bind(org_b.to_string()))
        .await
        .expect("set foreign GUC");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&mut *conn)
        .await
        .expect("count with foreign GUC");

    conn.execute(sqlx::query("RESET ROLE"))
        .await
        .expect("reset role");

    assert_eq!(
        count, 0,
        "Foreign org GUC must not reveal rows from another org"
    );
}

// ---------------------------------------------------------------------------
// 4. RLS is enabled on all required tenant tables
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn rls_enabled_on_all_tenant_tables(pool: PgPool) {
    let tables_with_rls: Vec<String> = sqlx::query_scalar(
        "SELECT relname::text FROM pg_class \
         WHERE relrowsecurity = TRUE AND relkind = 'r' \
         ORDER BY relname",
    )
    .fetch_all(&pool)
    .await
    .expect("query rls-enabled tables");

    for table in [
        "projects",
        "prompts",
        "prompt_runs",
        "mentions",
        "citations",
        "api_keys",
        "webhooks",
        "schedules",
        "recommendations",
        "alert_rules",
    ] {
        assert!(
            tables_with_rls.contains(&table.to_string()),
            "RLS not enabled on required tenant table: {table}"
        );
    }
}
