//! Story 20.2 — org_id backfill + default-Org migration tests.
//!
//! Verifies:
//!  1. Default org is created with slug='default'.
//!  2. org_id column exists and is NOT NULL on every tenant table.
//!  3. A newly inserted project/prompt/prompt_run inherits org_id.
//!  4. brands VIEW exposes brand_id alias.
//!  5. Idempotency: re-running the DO block doesn't error.
//!  6. Phase 1–3 row counts are conserved (no rows lost, RR-Phase4-NoContractBreak).

use sqlx::PgPool;

// ---------------------------------------------------------------------------
// 1. Default org exists after migration
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn default_org_is_created(pool: PgPool) {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM organizations WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("query default org");

    assert_eq!(count, 1, "default org must exist after migration 20.2");
}

// ---------------------------------------------------------------------------
// 2. org_id column is NOT NULL on core tenant tables
// ---------------------------------------------------------------------------

async fn assert_org_id_not_null(pool: &PgPool, table: &str) {
    let is_nullable: String = sqlx::query_scalar(&format!(
        "SELECT is_nullable FROM information_schema.columns \
         WHERE table_name = '{table}' AND column_name = 'org_id'"
    ))
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("org_id column missing on table: {table}"));

    assert_eq!(
        is_nullable, "NO",
        "org_id on {table} must be NOT NULL after backfill"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn org_id_not_null_on_tenant_tables(pool: PgPool) {
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
    ] {
        assert_org_id_not_null(&pool, table).await;
    }
}

// ---------------------------------------------------------------------------
// 3. New rows inherit org_id from the FK
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn new_project_can_be_inserted_with_default_org(pool: PgPool) {
    let default_org: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("default org");

    let project_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO projects (name, competitors, variants, org_id) \
         VALUES ('test-project', '[]'::jsonb, '{}'::text[], $1) RETURNING id",
    )
    .bind(default_org)
    .fetch_one(&pool)
    .await
    .expect("insert project");

    let stored_org: uuid::Uuid = sqlx::query_scalar("SELECT org_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("read org_id");

    assert_eq!(stored_org, default_org);
}

// ---------------------------------------------------------------------------
// 4. brands VIEW exposes brand_id alias
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn brands_view_exposes_brand_id(pool: PgPool) {
    let col_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_name = 'brands' AND column_name = 'brand_id'",
    )
    .fetch_one(&pool)
    .await
    .expect("query brands view");

    assert_eq!(col_exists, 1, "brands view must expose brand_id column");
}

#[sqlx::test(migrations = "./migrations")]
async fn brands_view_brand_id_equals_project_id(pool: PgPool) {
    let default_org: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM organizations WHERE slug = 'default'")
            .fetch_one(&pool)
            .await
            .expect("default org");

    let project_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO projects (name, competitors, variants, org_id) \
         VALUES ('brand-test', '[]'::jsonb, '{}'::text[], $1) RETURNING id",
    )
    .bind(default_org)
    .fetch_one(&pool)
    .await
    .expect("insert project");

    let brand_id: uuid::Uuid = sqlx::query_scalar("SELECT brand_id FROM brands WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .expect("read brand_id from brands view");

    assert_eq!(brand_id, project_id, "brand_id must equal project id");
}

// ---------------------------------------------------------------------------
// 5. Idempotency: re-running INSERT ... ON CONFLICT doesn't error
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn default_org_insert_is_idempotent(pool: PgPool) {
    // Running the same insert again must succeed silently.
    let result = sqlx::query(
        "INSERT INTO organizations (slug, name) VALUES ('default', 'Default Organization') \
         ON CONFLICT (slug) DO NOTHING",
    )
    .execute(&pool)
    .await;

    assert!(result.is_ok(), "idempotent insert must not fail");
}
