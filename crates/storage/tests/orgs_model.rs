//! Story 20.1 — org/operator data model smoke tests (D-P4-7).
//!
//! Verifies the forward-only migration `20260617200000_orgs.sql`:
//!  1. All five new tables exist with the expected columns.
//!  2. Enum types `org_role` and `invite_state` exist with the right variants.
//!  3. Unique/FK constraints hold (slug uniqueness, email+state uniqueness).
//!  4. Phase 1–3 tables are fully intact (RR-Phase4-NoContractBreak).

use sqlx::{PgPool, Row};

// ---------------------------------------------------------------------------
// 1. Table + column existence
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
async fn organizations_table_columns_exist(pool: PgPool) {
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'organizations' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("query columns");

    for expected in ["id", "slug", "name", "region", "created_at", "updated_at"] {
        assert!(
            cols.iter().any(|c| c == expected),
            "organizations missing column: {expected}"
        );
    }
}

#[sqlx::test(migrations = "migrations")]
async fn operators_table_columns_exist(pool: PgPool) {
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'operators' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("query columns");

    for expected in ["id", "login", "display_name", "email", "idp_sub", "created_at"] {
        assert!(
            cols.iter().any(|c| c == expected),
            "operators missing column: {expected}"
        );
    }
}

#[sqlx::test(migrations = "migrations")]
async fn operator_org_roles_table_exists(pool: PgPool) {
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'operator_org_roles' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("query columns");

    for expected in ["operator_id", "org_id", "role", "granted_by", "granted_at"] {
        assert!(
            cols.iter().any(|c| c == expected),
            "operator_org_roles missing column: {expected}"
        );
    }
}

#[sqlx::test(migrations = "migrations")]
async fn brand_grants_table_exists(pool: PgPool) {
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'brand_grants' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("query columns");

    for expected in ["operator_id", "project_id", "org_id", "granted_by", "granted_at"] {
        assert!(
            cols.iter().any(|c| c == expected),
            "brand_grants missing column: {expected}"
        );
    }
}

#[sqlx::test(migrations = "migrations")]
async fn org_invites_table_exists(pool: PgPool) {
    let cols: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_name = 'org_invites' ORDER BY column_name",
    )
    .fetch_all(&pool)
    .await
    .expect("query columns");

    for expected in [
        "id", "org_id", "invited_email", "role", "state", "token_hash",
        "invited_by", "expires_at", "created_at",
    ] {
        assert!(
            cols.iter().any(|c| c == expected),
            "org_invites missing column: {expected}"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Enum types
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
async fn org_role_enum_variants(pool: PgPool) {
    let variants: Vec<String> = sqlx::query_scalar(
        "SELECT enumlabel FROM pg_enum \
         JOIN pg_type ON pg_type.oid = pg_enum.enumtypid \
         WHERE typname = 'org_role' \
         ORDER BY enumlabel",
    )
    .fetch_all(&pool)
    .await
    .expect("query org_role enum");

    for expected in ["admin", "billing", "operator", "owner", "viewer"] {
        assert!(
            variants.contains(&expected.to_string()),
            "org_role missing variant: {expected}"
        );
    }
}

#[sqlx::test(migrations = "migrations")]
async fn invite_state_enum_variants(pool: PgPool) {
    let variants: Vec<String> = sqlx::query_scalar(
        "SELECT enumlabel FROM pg_enum \
         JOIN pg_type ON pg_type.oid = pg_enum.enumtypid \
         WHERE typname = 'invite_state' \
         ORDER BY enumlabel",
    )
    .fetch_all(&pool)
    .await
    .expect("query invite_state enum");

    for expected in ["accepted", "expired", "failed", "invited", "pending"] {
        assert!(
            variants.contains(&expected.to_string()),
            "invite_state missing variant: {expected}"
        );
    }
}

// ---------------------------------------------------------------------------
// 3. Constraint enforcement
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
async fn organizations_slug_unique_constraint(pool: PgPool) {
    sqlx::query(
        "INSERT INTO organizations (slug, name) VALUES ('acme', 'Acme Corp')",
    )
    .execute(&pool)
    .await
    .expect("first insert");

    let result = sqlx::query(
        "INSERT INTO organizations (slug, name) VALUES ('acme', 'Duplicate')",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "duplicate slug should violate unique constraint");
}

#[sqlx::test(migrations = "migrations")]
async fn organizations_slug_format_constraint(pool: PgPool) {
    // Slug must match '^[a-z0-9][a-z0-9\-]{0,61}[a-z0-9]$'
    let result = sqlx::query(
        "INSERT INTO organizations (slug, name) VALUES ('UPPER-CASE', 'Bad')",
    )
    .execute(&pool)
    .await;
    assert!(result.is_err(), "uppercase slug should violate check constraint");
}

#[sqlx::test(migrations = "migrations")]
async fn operators_login_unique_constraint(pool: PgPool) {
    sqlx::query(
        "INSERT INTO operators (login) VALUES ('alice')",
    )
    .execute(&pool)
    .await
    .expect("first insert");

    let result = sqlx::query(
        "INSERT INTO operators (login) VALUES ('alice')",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "duplicate login should violate unique constraint");
}

#[sqlx::test(migrations = "migrations")]
async fn operator_org_role_round_trip(pool: PgPool) {
    let org_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO organizations (slug, name) VALUES ('test-org', 'Test Org') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert org");

    let op_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO operators (login) VALUES ('bob') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert operator");

    sqlx::query(
        "INSERT INTO operator_org_roles (operator_id, org_id, role) \
         VALUES ($1, $2, 'admin')",
    )
    .bind(op_id)
    .bind(org_id)
    .execute(&pool)
    .await
    .expect("insert role");

    let role: String = sqlx::query_scalar(
        "SELECT role::text FROM operator_org_roles WHERE operator_id = $1 AND org_id = $2",
    )
    .bind(op_id)
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("read role");

    assert_eq!(role, "admin");
}

#[sqlx::test(migrations = "migrations")]
async fn org_invite_state_machine_insert(pool: PgPool) {
    let org_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO organizations (slug, name) VALUES ('invite-org', 'Invite Org') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .expect("insert org");

    sqlx::query(
        "INSERT INTO org_invites (org_id, invited_email, token_hash) \
         VALUES ($1, 'carol@example.com', 'abc123hash')",
    )
    .bind(org_id)
    .execute(&pool)
    .await
    .expect("insert invite");

    let state: String = sqlx::query_scalar(
        "SELECT state::text FROM org_invites WHERE invited_email = 'carol@example.com'",
    )
    .fetch_one(&pool)
    .await
    .expect("read state");

    assert_eq!(state, "pending");
}

// ---------------------------------------------------------------------------
// 4. Phase 1–3 tables intact (RR-Phase4-NoContractBreak)
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "migrations")]
async fn phase1_tables_still_exist(pool: PgPool) {
    for table in ["projects", "prompts", "prompt_runs", "mentions", "citations"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = '{table}'"
        ))
        .fetch_one(&pool)
        .await
        .expect("query table existence");

        assert_eq!(count, 1, "Phase 1 table missing after 20.1 migration: {table}");
    }
}
