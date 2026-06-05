//! Dual-backend gate (Story 37.4, AC4): the SAME storage migrations + `.sqlx`
//! cache that run against a Tier-2 external Postgres must also run green against
//! the managed CHILD Postgres provisioned by [`anseo_cli::datastore`].
//!
//! The rest of CI exercises the EXTERNAL backend (the `postgres:` service with
//! `DATABASE_URL` set). This test exercises the OTHER half: it boots a real
//! managed child via the production [`PgCliSupervisor`] (initdb → pg_ctl over a
//! Unix socket, no TCP), connects with the derived `DATABASE_URL`, and applies
//! `anseo_storage`'s embedded migrations. If they apply cleanly the schema is
//! proven portable across both backends on one set of migration files.
//!
//! ## Why env-gated rather than `#[ignore]`
//!
//! Booting a real Postgres downloads/needs server binaries and is slow, so it
//! must NOT run in the default `cargo test --workspace`. It is gated on
//! `ANSEO_TEST_MANAGED_PG=1`, which the dedicated `rust-managed-pg` CI job sets
//! after putting `initdb`/`pg_ctl` on PATH. Locally: run it explicitly with
//! `ANSEO_TEST_MANAGED_PG=1 cargo test -p anseo-cli --test managed_pg_migrations`.

use anseo_cli::datastore::{ChildPostgres, DataLayout, Datastore, PgCliSupervisor};

/// The env switch that opts a runner into the (slow, binary-dependent) boot.
const ENABLE_ENV: &str = "ANSEO_TEST_MANAGED_PG";

/// Boot a managed child Postgres and run `anseo_storage`'s migrations against
/// it, proving AC4's dual-backend portability for the managed half.
#[test]
fn managed_child_runs_storage_migrations() {
    if std::env::var(ENABLE_ENV).ok().as_deref() != Some("1") {
        eprintln!(
            "skipping managed-child-Postgres migration gate; set {ENABLE_ENV}=1 to enable \
             (requires initdb/pg_ctl on PATH). The dedicated CI job runs this."
        );
        return;
    }

    // An isolated cluster under a temp root — never the shared machine store, so
    // the test is hermetic and leaves nothing behind.
    let tmp = tempfile::tempdir().expect("tempdir");
    let layout = DataLayout::derive(Some(tmp.path().to_string_lossy().into_owned()), None)
        .expect("derive layout");
    let supervisor = PgCliSupervisor::new(layout.clone());

    // Provision (initdb) + start (pg_ctl, Unix socket). Stop on every exit path
    // below via the guard so a failed assertion never leaks a child.
    supervisor.provision().expect("provision child Postgres");
    assert!(
        layout.is_initialised(),
        "data dir should be initialised after provision"
    );
    let database_url = supervisor.start().expect("start child Postgres");
    let _guard = StopGuard(&supervisor);

    // Connect with the EXACT URL the production resolver hands callers, then run
    // the embedded migrations — the same files the external-Postgres CI job uses.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(async {
        let storage = anseo_storage::Storage::connect(&database_url)
            .await
            .expect("connect to managed child over its Unix socket");
        storage
            .migrate()
            .await
            .expect("storage migrations apply cleanly on the managed child");
        // Idempotent re-run: applying twice is a no-op (forward-only contract).
        storage
            .migrate()
            .await
            .expect("re-running migrations on the managed child is idempotent");
    });
}

/// Confirms the resolver's no-`DATABASE_URL` path yields a `Managed` datastore
/// whose URL is the same socket URL the supervisor derives — i.e. the wiring
/// `serve` relies on is exercised, not just the supervisor in isolation.
#[test]
fn resolve_with_no_database_url_yields_managed_child() {
    if std::env::var(ENABLE_ENV).ok().as_deref() != Some("1") {
        eprintln!("skipping managed-child resolve gate; set {ENABLE_ENV}=1 to enable.");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let layout =
        DataLayout::derive(Some(tmp.path().to_string_lossy().into_owned()), None).expect("layout");
    let expected = layout.database_url();
    let supervisor = PgCliSupervisor::new(layout);

    let datastore = anseo_cli::datastore::resolve_datastore(None, Box::new(supervisor))
        .expect("resolve managed child");
    assert!(matches!(datastore, Datastore::Managed(_)));
    assert_eq!(datastore.database_url(), expected);
    // Dropping `datastore` stops the child via ManagedHandle::drop.
}

/// Stops the child on drop so a panicking assertion never leaves a running
/// Postgres behind. Best-effort: a stop failure is logged, not re-panicked.
struct StopGuard<'a>(&'a PgCliSupervisor);

impl Drop for StopGuard<'_> {
    fn drop(&mut self) {
        if let Err(err) = self.0.stop() {
            eprintln!("warning: failed to stop managed child Postgres in test teardown: {err}");
        }
    }
}
