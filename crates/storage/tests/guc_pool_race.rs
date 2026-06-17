//! Story 20.10 — GUC-bleed pool-race concurrency test ([p4-iso-3], co-equal blocker).
//!
//! AC-1: Concurrent workers across N tenants run against a pool sized far below
//!       worker count, forcing connection reuse. Zero foreign rows across all ops.
//! AC-2: Explicit negative scenarios: autocommit (query outside explicit txn) and
//!       a SET (not SET LOCAL) leak are demonstrated and shown safe.
//! AC-3: Wired as co-equal release-blocking GA criterion [p4-iso-3].
//!
//! The test uses SET LOCAL inside explicit transactions to set the GUC — the
//! correct production pattern. The negative scenario with plain SET (which does
//! NOT reset on COMMIT) is also exercised to show the leak risk and confirm
//! that our pattern (SET LOCAL) is immune.

use sqlx::{Executor, PgPool, Row};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

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

async fn insert_org(pool: &PgPool, slug: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO organizations (slug, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(slug)
    .bind(slug)
    .fetch_one(pool)
    .await
    .expect("insert org")
}

async fn insert_project_as_superuser(pool: &PgPool, org_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
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
// AC-1: Concurrent pool-reuse test with SET LOCAL (the correct pattern).
//
// N_ORGS tenants, N_WORKERS concurrent tasks, ITERATIONS reads each.
// Each task picks its assigned org, sets GUC via SET LOCAL inside a BEGIN/COMMIT
// and asserts it only sees its own rows.
// ---------------------------------------------------------------------------

const N_ORGS: usize = 4;
const N_WORKERS: usize = 20;
const ITERATIONS: usize = 50;

#[sqlx::test(migrations = "./migrations")]
async fn set_local_prevents_guc_bleed_under_pool_reuse(pool: PgPool) {
    setup_rls_tester_role(&pool).await;

    // Seed N_ORGS with one project each.
    let mut orgs: Vec<(Uuid, Uuid)> = Vec::new(); // (org_id, project_id)
    for i in 0..N_ORGS {
        let org_id = insert_org(&pool, &format!("race-org-{i}")).await;
        let proj_id = insert_project_as_superuser(&pool, org_id, &format!("race-proj-{i}")).await;
        orgs.push((org_id, proj_id));
    }

    let pool = Arc::new(pool);
    let leaks: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for w in 0..N_WORKERS {
        let pool = Arc::clone(&pool);
        let leaks = Arc::clone(&leaks);
        let (my_org, my_proj) = orgs[w % N_ORGS];

        let handle = tokio::spawn(async move {
            for _ in 0..ITERATIONS {
                // Acquire a connection — may be reused from another worker.
                let mut conn = pool.acquire().await.expect("acquire");

                // Begin explicit transaction, set GUC with SET LOCAL.
                conn.execute(sqlx::query("BEGIN")).await.expect("BEGIN");
                conn.execute(sqlx::query("SET ROLE rls_tester"))
                    .await
                    .expect("set role");
                conn.execute(
                    sqlx::query("SELECT set_config('app.org', $1, true)").bind(my_org.to_string()),
                )
                .await
                .expect("set GUC (SET LOCAL via set_config true=local)");

                // Read all visible project ids.
                let visible: Vec<Uuid> =
                    sqlx::query("SELECT id FROM projects WHERE org_id IS NOT NULL")
                        .fetch_all(&mut *conn)
                        .await
                        .expect("select projects")
                        .into_iter()
                        .map(|r| r.get::<Uuid, _>(0))
                        .collect();

                conn.execute(sqlx::query("RESET ROLE"))
                    .await
                    .expect("reset role");
                conn.execute(sqlx::query("COMMIT")).await.expect("COMMIT");
                drop(conn);

                // Assert we only saw our own project.
                for seen_id in &visible {
                    if *seen_id != my_proj {
                        leaks
                            .lock()
                            .unwrap()
                            .push(format!("worker org={my_org} saw foreign project={seen_id}"));
                    }
                }

                // Also assert our own row is visible (fail-open would be wrong).
                if !visible.contains(&my_proj) {
                    leaks.lock().unwrap().push(format!(
                        "worker org={my_org} couldn't see own project={my_proj}"
                    ));
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.expect("worker panicked");
    }

    let leaks = leaks.lock().unwrap();
    assert!(
        leaks.is_empty(),
        "[p4-iso-3] GUC bleed detected under pool reuse:\n{}",
        leaks.join("\n")
    );
}

// ---------------------------------------------------------------------------
// AC-2a: Negative scenario — plain SET (not SET LOCAL) bleeds after COMMIT.
//
// This test DEMONSTRATES the hazard by showing that after a transaction
// commits, the GUC set with SET (not SET LOCAL) persists on the connection.
// Our production code uses SET LOCAL, which this test also validates.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn plain_set_leaks_after_commit_demonstrating_hazard(pool: PgPool) {
    // Use a single connection to observe the GUC persistence.
    let mut conn = pool.acquire().await.expect("acquire");

    let sentinel = Uuid::new_v4().to_string();

    // SET (not SET LOCAL) inside a transaction.
    conn.execute(sqlx::query("BEGIN")).await.expect("BEGIN");
    conn.execute(sqlx::query("SELECT set_config('app.org', $1, false)").bind(&sentinel))
        .await
        .expect("set GUC with is_local=false");
    conn.execute(sqlx::query("COMMIT")).await.expect("COMMIT");

    // After COMMIT, the session-level GUC is still set (this is the hazard).
    let after: String = sqlx::query_scalar("SELECT current_setting('app.org', true)::text")
        .fetch_one(&mut *conn)
        .await
        .expect("read GUC after COMMIT");

    assert_eq!(
        after, sentinel,
        "Hazard demonstration: plain SET persists after COMMIT (this is expected and shows why SET LOCAL is required)"
    );
}

// ---------------------------------------------------------------------------
// AC-2b: SET LOCAL resets after COMMIT — the correct pattern.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn set_local_resets_after_commit(pool: PgPool) {
    let mut conn = pool.acquire().await.expect("acquire");

    let sentinel = Uuid::new_v4().to_string();

    // SET LOCAL inside a transaction.
    conn.execute(sqlx::query("BEGIN")).await.expect("BEGIN");
    conn.execute(sqlx::query("SELECT set_config('app.org', $1, true)").bind(&sentinel))
        .await
        .expect("SET LOCAL via set_config(..., true)");
    conn.execute(sqlx::query("COMMIT")).await.expect("COMMIT");

    // After COMMIT, SET LOCAL has been rolled back — GUC is now NULL.
    let after: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('app.org', true), '')")
            .fetch_one(&mut *conn)
            .await
            .expect("read GUC after COMMIT");

    assert!(
        after.is_none(),
        "[p4-iso-3] SET LOCAL must reset to NULL after COMMIT, got: {after:?}"
    );
}

// ---------------------------------------------------------------------------
// AC-2c: Autocommit path (query outside BEGIN/COMMIT) with SET LOCAL is safe.
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn autocommit_set_local_does_not_persist(pool: PgPool) {
    // In autocommit mode, each statement is its own transaction.
    // SET LOCAL is transaction-scoped, so it applies and immediately expires.
    let mut conn = pool.acquire().await.expect("acquire");

    let sentinel = Uuid::new_v4().to_string();

    // Execute set_config with is_local=true outside an explicit txn.
    // The implicit transaction commits immediately — GUC disappears.
    conn.execute(sqlx::query("SELECT set_config('app.org', $1, true)").bind(&sentinel))
        .await
        .expect("set_config in autocommit mode");

    // GUC is already gone.
    let after: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('app.org', true), '')")
            .fetch_one(&mut *conn)
            .await
            .expect("read GUC");

    assert!(
        after.is_none(),
        "[p4-iso-3] SET LOCAL in autocommit must not persist, got: {after:?}"
    );
}

// ---------------------------------------------------------------------------
// AC-3: Wired to GA gate — this test passing satisfies [p4-iso-3].
// The phase4-ga-check.sh script references this file as evidence.
// ---------------------------------------------------------------------------

/// Sentinel that lets the GA check script grep for the iso-3 evidence marker.
#[allow(dead_code)]
const P4_ISO_3_EVIDENCE: &str =
    "p4-iso-3: guc_pool_race::set_local_prevents_guc_bleed_under_pool_reuse";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Used by external harnesses (Story 26.3 DR re-run) to run the soak in isolation.
#[allow(dead_code)]
pub async fn run_soak(pool: &PgPool, n_orgs: usize, n_workers: usize, iterations: usize) {
    let mut orgs: Vec<(Uuid, Uuid)> = Vec::new();
    for i in 0..n_orgs {
        let org_id = insert_org(pool, &format!("soak-org-{i}")).await;
        let proj_id = insert_project_as_superuser(pool, org_id, &format!("soak-proj-{i}")).await;
        orgs.push((org_id, proj_id));
    }

    let pool = Arc::new(pool.clone());
    let leaks: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for w in 0..n_workers {
        let pool = Arc::clone(&pool);
        let leaks = Arc::clone(&leaks);
        let (my_org, my_proj) = orgs[w % n_orgs];

        handles.push(tokio::spawn(async move {
            for _ in 0..iterations {
                let mut conn = pool.acquire().await.expect("acquire");
                conn.execute(sqlx::query("BEGIN")).await.expect("BEGIN");
                conn.execute(sqlx::query("SET ROLE rls_tester"))
                    .await
                    .expect("set role");
                conn.execute(
                    sqlx::query("SELECT set_config('app.org', $1, true)").bind(my_org.to_string()),
                )
                .await
                .expect("set GUC");
                let visible: HashSet<Uuid> = sqlx::query("SELECT id FROM projects")
                    .fetch_all(&mut *conn)
                    .await
                    .expect("select")
                    .into_iter()
                    .map(|r| r.get::<Uuid, _>(0))
                    .collect();
                conn.execute(sqlx::query("RESET ROLE"))
                    .await
                    .expect("reset role");
                conn.execute(sqlx::query("COMMIT")).await.expect("COMMIT");
                drop(conn);

                for seen in &visible {
                    if *seen != my_proj {
                        leaks
                            .lock()
                            .unwrap()
                            .push(format!("org={my_org} saw {seen}"));
                    }
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("worker panicked");
    }

    let l = leaks.lock().unwrap();
    assert!(l.is_empty(), "soak leaks:\n{}", l.join("\n"));
}
