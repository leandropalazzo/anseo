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

use sqlx::{Executor, PgPool};

// ---------------------------------------------------------------------------
// Helper: insert a row into `projects` with a given org_id, bypassing RLS
// via SET LOCAL to the correct org first.
// ---------------------------------------------------------------------------
async fn insert_project_for_org(pool: &PgPool, org_id: uuid::Uuid, name: &str) -> uuid::Uuid {
    let mut conn = pool.acquire().await.expect("acquire connection");
    // Temporarily set the GUC so INSERT passes RLS WITH CHECK.
    conn.execute(sqlx::query("SET LOCAL app.org = $1::text").bind(org_id.to_string()))
        .await
        .expect("set GUC for insert");

    sqlx::query_scalar::<_, uuid::Uuid>(
        "INSERT INTO projects (name, competitors, variants, org_id) \
         VALUES ($1, '[]'::jsonb, '{}'::text[], $2) RETURNING id",
    )
    .bind(name)
    .bind(org_id)
    .fetch_one(&mut *conn)
    .await
    .expect("insert project")
}

// ---------------------------------------------------------------------------
// 1. Unset GUC → zero rows on projects
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
async fn unset_guc_yields_zero_rows(pool: PgPool) {
    // Get the default org to insert a row.
    let default_org: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("default org");

    // Insert a project row for the default org.
    insert_project_for_org(&pool, default_org, "rls-test-project").await;

    // Now query without setting any app.org GUC — RLS must yield zero rows.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&pool)
        .await
        .expect("count projects without GUC");

    assert_eq!(
        count, 0,
        "Unset app.org GUC must yield zero rows (fail-closed, RR-Phase4-RlsFailClosed)"
    );
}

// ---------------------------------------------------------------------------
// 2. Correct org GUC → own row visible
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
async fn correct_org_guc_yields_own_rows(pool: PgPool) {
    let default_org: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("default org");

    insert_project_for_org(&pool, default_org, "visible-project").await;

    // Set GUC to the correct org in the current session.
    sqlx::query("SET app.org = $1::text")
        .bind(default_org.to_string())
        .execute(&pool)
        .await
        .expect("set GUC");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&pool)
        .await
        .expect("count with correct GUC");

    assert_eq!(count, 1, "Correct org GUC must reveal own rows");

    // Reset GUC.
    sqlx::query("RESET app.org").execute(&pool).await.ok();
}

// ---------------------------------------------------------------------------
// 3. Foreign org GUC → foreign row not visible
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
async fn foreign_org_guc_yields_zero_rows(pool: PgPool) {
    // Create two orgs.
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

    // Insert a project for org_a.
    insert_project_for_org(&pool, org_a, "org-a-project").await;

    // Set GUC to org_b — should see zero rows from org_a.
    sqlx::query("SET app.org = $1::text")
        .bind(org_b.to_string())
        .execute(&pool)
        .await
        .expect("set foreign GUC");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projects")
        .fetch_one(&pool)
        .await
        .expect("count with foreign GUC");

    assert_eq!(
        count, 0,
        "Foreign org GUC must not reveal rows from another org"
    );

    sqlx::query("RESET app.org").execute(&pool).await.ok();
}

// ---------------------------------------------------------------------------
// 4. RLS is enabled on all required tenant tables
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
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
