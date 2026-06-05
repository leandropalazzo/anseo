//! Managed child Postgres datastore (Story 37.4, ADR-005).
//!
//! When no `DATABASE_URL` is set, `ogeo serve` (and the direct-DB CLI verbs)
//! provision and supervise a CHILD upstream Postgres so an operator needs zero
//! setup. The storage layer is Postgres-specific, so a SQLite `DATABASE_URL` is
//! rejected outright. There is one shared store per machine, under
//! `$XDG_DATA_HOME/opengeo` (falling back to `$HOME/.local/share/opengeo`).
//!
//! ## Lifecycle (behind the [`ChildPostgres`] trait)
//!
//! `provision` (initdb into the data dir) → `start` (bind a Unix socket, NOT
//! TCP) → health-check → derive a `DATABASE_URL` → `stop` on shutdown. Every
//! step is idempotent: an existing initialised data dir is reused.
//!
//! ## Override short-circuit
//!
//! [`resolve_datastore`] is the single decision seam. When `DATABASE_URL` is
//! set the child is NEVER started and behavior is unchanged; only its absence
//! takes the auto-provision path.
//!
//! ## Test split (hermetic vs ignored)
//!
//! All the LOGIC — path derivation, the override short-circuit, socket-path
//! derivation, the idempotent-reuse decision, SQLite rejection, shutdown wiring
//! — is unit-tested hermetically (no network, no real PG). The single test that
//! actually boots a real child Postgres is `#[ignore]`d so CI never downloads
//! binaries or boots PG over the network; it shells out to an `initdb` /
//! `pg_ctl` discovered on `PATH` when run explicitly.

use std::path::{Path, PathBuf};

use anseo_core::OpenGeoError;

/// Name of the env var that, when present, points at an EXTERNAL Postgres and
/// short-circuits the managed child entirely.
const DATABASE_URL_ENV: &str = "DATABASE_URL";

/// The single shared data root lives under this subdirectory of the platform
/// data dir, so every invocation on a machine reuses one store.
const DATA_SUBDIR: &str = "opengeo";

/// Default Postgres database name created inside the managed cluster.
const MANAGED_DB_NAME: &str = "opengeo";

/// The resolved datastore: either an operator-supplied external URL, or a
/// supervised managed child whose lifetime is tied to the returned handle.
pub enum Datastore {
    /// `DATABASE_URL` was set; the child is NOT started. Behavior is unchanged.
    External(String),
    /// No `DATABASE_URL`; a managed child Postgres is running. Dropping the
    /// handle stops it.
    Managed(ManagedHandle),
}

impl Datastore {
    /// The `DATABASE_URL` callers should connect with, regardless of source.
    pub fn database_url(&self) -> &str {
        match self {
            Datastore::External(url) => url,
            Datastore::Managed(handle) => &handle.database_url,
        }
    }
}

/// A running managed child Postgres. `Drop` stops the process so a normal
/// shutdown (or an early return / panic on the way up) never leaks a child.
pub struct ManagedHandle {
    database_url: String,
    supervisor: Box<dyn ChildPostgres>,
}

impl ManagedHandle {
    /// The derived connection URL for the managed cluster.
    pub fn database_url(&self) -> &str {
        &self.database_url
    }
}

impl Drop for ManagedHandle {
    fn drop(&mut self) {
        // Best-effort: log but never panic in Drop.
        if let Err(err) = self.supervisor.stop() {
            tracing::warn!(event = "datastore.stop_failed", error = %err, "failed to stop managed child Postgres cleanly");
        }
    }
}

/// The lifecycle abstraction for a child Postgres. Implementations own the
/// actual process; the resolver only drives this seam, which keeps the decision
/// logic unit-testable against a fake.
pub trait ChildPostgres: Send + Sync {
    /// `initdb` into the data dir if it is not already initialised. Idempotent.
    fn provision(&self) -> Result<(), OpenGeoError>;
    /// Start the server bound to a Unix socket (NOT TCP) and block until it
    /// passes a health check. Returns the derived `DATABASE_URL`.
    fn start(&self) -> Result<String, OpenGeoError>;
    /// Stop the running server. Idempotent; safe to call from `Drop`.
    fn stop(&self) -> Result<(), OpenGeoError>;
}

/// Resolve the datastore for a command run.
///
/// This is the override short-circuit: if `DATABASE_URL` is set it is returned
/// verbatim (after rejecting SQLite) and `provision`/`start` are NEVER called.
/// Otherwise the supplied supervisor is provisioned and started.
pub fn resolve_datastore(
    env_database_url: Option<String>,
    supervisor: Box<dyn ChildPostgres>,
) -> Result<Datastore, OpenGeoError> {
    match env_database_url {
        Some(url) => {
            reject_sqlite(&url)?;
            Ok(Datastore::External(url))
        }
        None => {
            supervisor.provision()?;
            let database_url = supervisor.start()?;
            Ok(Datastore::Managed(ManagedHandle {
                database_url,
                supervisor,
            }))
        }
    }
}

/// Convenience entry point: read `DATABASE_URL` from the process environment and
/// resolve against a real on-PATH child Postgres supervisor.
pub fn resolve_from_env() -> Result<Datastore, OpenGeoError> {
    let env = read_database_url_env();
    let layout = DataLayout::from_env()?;
    resolve_datastore(env, Box::new(PgCliSupervisor::new(layout)))
}

/// Read `DATABASE_URL`, treating an empty value as unset.
pub fn read_database_url_env() -> Option<String> {
    match std::env::var(DATABASE_URL_ENV) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

/// Reject SQLite URLs — the storage layer is Postgres-specific.
pub fn reject_sqlite(url: &str) -> Result<(), OpenGeoError> {
    let lower = url.trim().to_ascii_lowercase();
    if lower.starts_with("sqlite:") || lower.starts_with("sqlite3:") {
        return Err(OpenGeoError::Config(
            "SQLite is not supported; OpenGEO requires PostgreSQL. Set DATABASE_URL to a postgres:// URL or unset it to use the managed datastore.".into(),
        ));
    }
    Ok(())
}

/// On-disk layout for the single shared managed cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataLayout {
    /// `$XDG_DATA_HOME/opengeo` (or the `$HOME` fallback).
    root: PathBuf,
}

impl DataLayout {
    /// Derive the layout from the environment.
    ///
    /// `$XDG_DATA_HOME` wins when set to an absolute path; otherwise we fall
    /// back to `$HOME/.local/share`. Both are read here so the pure derivation
    /// in [`DataLayout::derive`] stays unit-testable without process env.
    pub fn from_env() -> Result<Self, OpenGeoError> {
        Self::derive(
            std::env::var("XDG_DATA_HOME").ok(),
            std::env::var("HOME").ok(),
        )
    }

    /// Pure derivation used by [`DataLayout::from_env`] and the unit tests.
    pub fn derive(
        xdg_data_home: Option<String>,
        home: Option<String>,
    ) -> Result<Self, OpenGeoError> {
        let base = match xdg_data_home {
            Some(x) if Path::new(&x).is_absolute() => PathBuf::from(x),
            _ => {
                let home = home.ok_or_else(|| {
                    OpenGeoError::Config(
                        "cannot locate the data directory: neither XDG_DATA_HOME nor HOME is set"
                            .into(),
                    )
                })?;
                PathBuf::from(home).join(".local").join("share")
            }
        };
        Ok(Self {
            root: base.join(DATA_SUBDIR),
        })
    }

    /// The Postgres cluster data directory (`initdb -D` target).
    pub fn data_dir(&self) -> PathBuf {
        self.root.join("pgdata")
    }

    /// The directory the Unix domain socket is created in. Postgres connects via
    /// `host=<dir>`; we keep it short and under our own root.
    pub fn socket_dir(&self) -> PathBuf {
        self.root.join("run")
    }

    /// A data dir counts as initialised once `PG_VERSION` exists in it — the
    /// idempotent-reuse decision.
    pub fn is_initialised(&self) -> bool {
        self.data_dir().join("PG_VERSION").exists()
    }

    /// Derive the connection URL for the managed cluster over its Unix socket.
    ///
    /// The percent-encoded socket directory goes in the AUTHORITY host position
    /// (`postgres://user@%2Fpath%2Frun/db`), not a `?host=` query param. Both
    /// are valid for libpq, but sqlx's URL parser rejects an empty authority
    /// host (`EmptyHost`), so the socket dir must live in the authority for the
    /// pool to connect. Connects as the current OS user with no password.
    pub fn database_url(&self) -> String {
        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "postgres".to_string());
        let socket = self.socket_dir();
        format!(
            "postgres://{user}@{host}/{db}",
            user = user,
            host = encode_host(&socket.to_string_lossy()),
            db = MANAGED_DB_NAME,
        )
    }
}

/// Percent-encode the bits of a socket path that would break a URL query value.
fn encode_host(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'/' => out.push_str("%2F"),
            b' ' => out.push_str("%20"),
            b'&' => out.push_str("%26"),
            b'?' => out.push_str("%3F"),
            b'#' => out.push_str("%23"),
            _ => out.push(b as char),
        }
    }
    out
}

/// The locale environment forced onto every spawned Postgres tool.
///
/// `initdb`, the postmaster, and `createdb` all read `LC_*`/`LANG` from the
/// environment. On minimal or misconfigured hosts (fresh CI runners, slim
/// containers, a shell with an unset/invalid locale) those values abort
/// provisioning ("invalid locale") or refuse to start the postmaster
/// ("postmaster became multithreaded during startup; set LC_ALL"). Pinning the
/// always-available portable `C` locale makes the managed child boot
/// deterministically regardless of the operator's environment — the zero-setup
/// first-run guarantee. Operators who set `DATABASE_URL` bypass this entirely.
fn deterministic_locale_env() -> [(&'static str, &'static str); 2] {
    [("LC_ALL", "C"), ("LANG", "C")]
}

/// Real supervisor that shells out to an `initdb` / `pg_ctl` / `createdb`
/// discovered on `PATH`. Deliberately holds no async runtime so it can be driven
/// from both the async `serve` path and the synchronous direct-DB verbs.
pub struct PgCliSupervisor {
    layout: DataLayout,
}

impl PgCliSupervisor {
    pub fn new(layout: DataLayout) -> Self {
        Self { layout }
    }

    /// The layout this supervisor manages.
    pub fn layout(&self) -> &DataLayout {
        &self.layout
    }

    fn run_tool(&self, tool: &str, args: &[&std::ffi::OsStr]) -> Result<(), OpenGeoError> {
        let status = std::process::Command::new(tool)
            .args(args)
            .envs(deterministic_locale_env())
            .status()
            .map_err(|e| {
                OpenGeoError::Config(format!(
                    "`{tool}` not found on PATH or failed to start: {e}. Install PostgreSQL server binaries or set DATABASE_URL to an external Postgres."
                ))
            })?;
        if !status.success() {
            return Err(OpenGeoError::Config(format!(
                "`{tool}` exited with status {}",
                status.code().unwrap_or(-1)
            )));
        }
        Ok(())
    }
}

impl ChildPostgres for PgCliSupervisor {
    fn provision(&self) -> Result<(), OpenGeoError> {
        std::fs::create_dir_all(self.layout.socket_dir())
            .map_err(|e| OpenGeoError::Config(format!("failed to create socket dir: {e}")))?;
        if self.layout.is_initialised() {
            return Ok(()); // Idempotent reuse.
        }
        let data_dir = self.layout.data_dir();
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| OpenGeoError::Config(format!("failed to create data dir: {e}")))?;
        // Pin encoding + locale explicitly. Without this `initdb` inherits the
        // operator's `LANG`/`LC_*`, which on minimal/locale-less hosts (fresh
        // CI runners, slim containers) is invalid and aborts the very
        // zero-setup first run this datastore promises. `UTF8` + the portable
        // `C` locale are always available and make provisioning deterministic.
        self.run_tool(
            "initdb",
            &[
                std::ffi::OsStr::new("-D"),
                data_dir.as_os_str(),
                std::ffi::OsStr::new("--auth=trust"),
                std::ffi::OsStr::new("--no-sync"),
                std::ffi::OsStr::new("--encoding=UTF8"),
                std::ffi::OsStr::new("--locale=C"),
            ],
        )
    }

    fn start(&self) -> Result<String, OpenGeoError> {
        let socket_dir = self.layout.socket_dir();
        // Start via pg_ctl: bind ONLY a Unix socket (listen_addresses = ''),
        // never a TCP port. `-w` makes pg_ctl block until the server is ready,
        // which is the health check.
        let options = format!(
            "-c listen_addresses='' -c unix_socket_directories='{}'",
            socket_dir.display()
        );
        self.run_tool(
            "pg_ctl",
            &[
                std::ffi::OsStr::new("-D"),
                self.layout.data_dir().as_os_str(),
                std::ffi::OsStr::new("-o"),
                std::ffi::OsStr::new(&options),
                std::ffi::OsStr::new("-w"),
                std::ffi::OsStr::new("start"),
            ],
        )?;
        // Ensure the application database exists (idempotent — ignore the error
        // when it already exists from a prior run).
        let _ = std::process::Command::new("createdb")
            .arg("-h")
            .arg(&socket_dir)
            .arg(MANAGED_DB_NAME)
            .envs(deterministic_locale_env())
            .status();
        Ok(self.layout.database_url())
    }

    fn stop(&self) -> Result<(), OpenGeoError> {
        self.run_tool(
            "pg_ctl",
            &[
                std::ffi::OsStr::new("-D"),
                self.layout.data_dir().as_os_str(),
                std::ffi::OsStr::new("-m"),
                std::ffi::OsStr::new("fast"),
                std::ffi::OsStr::new("stop"),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ---- Path / layout derivation (hermetic) --------------------------------

    #[test]
    fn xdg_data_home_wins_when_absolute() {
        let layout = DataLayout::derive(Some("/data/xdg".into()), Some("/home/me".into())).unwrap();
        assert_eq!(layout.data_dir(), PathBuf::from("/data/xdg/opengeo/pgdata"));
        assert_eq!(layout.socket_dir(), PathBuf::from("/data/xdg/opengeo/run"));
    }

    #[test]
    fn falls_back_to_home_local_share() {
        let layout = DataLayout::derive(None, Some("/home/me".into())).unwrap();
        assert_eq!(
            layout.data_dir(),
            PathBuf::from("/home/me/.local/share/opengeo/pgdata")
        );
    }

    #[test]
    fn relative_xdg_is_ignored_in_favour_of_home() {
        // A non-absolute XDG_DATA_HOME is unsafe to trust; fall back to HOME.
        let layout =
            DataLayout::derive(Some("relative/path".into()), Some("/home/me".into())).unwrap();
        assert_eq!(
            layout.data_dir(),
            PathBuf::from("/home/me/.local/share/opengeo/pgdata")
        );
    }

    #[test]
    fn missing_both_is_a_config_error() {
        let err = DataLayout::derive(None, None).unwrap_err();
        assert!(matches!(err, OpenGeoError::Config(_)));
    }

    #[test]
    fn socket_dir_is_under_root_not_tcp() {
        let layout = DataLayout::derive(Some("/d".into()), None).unwrap();
        // Socket dir derivation: lives under our own root.
        assert!(layout.socket_dir().starts_with("/d/opengeo"));
    }

    #[test]
    fn database_url_uses_socket_host_not_tcp() {
        let layout = DataLayout::derive(Some("/d".into()), None).unwrap();
        let url = layout.database_url();
        // No TCP host:port; the percent-encoded socket dir is the authority host
        // (sqlx rejects an empty authority host), followed by the db path.
        assert!(url.starts_with("postgres://"));
        assert!(url.contains("@%2Fd%2Fopengeo%2Frun/opengeo"));
        assert!(!url.contains(":5432"));
        assert!(!url.contains("?host="));
    }

    #[test]
    fn encode_host_escapes_slashes() {
        assert_eq!(encode_host("/a/b"), "%2Fa%2Fb");
    }

    #[test]
    fn deterministic_locale_env_pins_portable_c_locale() {
        // Every spawned Postgres tool must run under the portable `C` locale so
        // a host with an unset/invalid `LC_*`/`LANG` cannot abort provisioning
        // or the postmaster. Both vars are pinned to `C`.
        let env = deterministic_locale_env();
        assert!(env.contains(&("LC_ALL", "C")));
        assert!(env.contains(&("LANG", "C")));
    }

    // ---- Idempotent-reuse decision (hermetic) -------------------------------

    #[test]
    fn is_initialised_tracks_pg_version_file() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = DataLayout {
            root: tmp.path().to_path_buf(),
        };
        assert!(!layout.is_initialised());
        std::fs::create_dir_all(layout.data_dir()).unwrap();
        assert!(!layout.is_initialised());
        std::fs::write(layout.data_dir().join("PG_VERSION"), "16").unwrap();
        assert!(layout.is_initialised());
    }

    // ---- SQLite rejection (hermetic) ----------------------------------------

    #[test]
    fn rejects_sqlite_urls() {
        assert!(reject_sqlite("sqlite::memory:").is_err());
        assert!(reject_sqlite("sqlite:/tmp/x.db").is_err());
        assert!(reject_sqlite("SQLite3://x").is_err());
        assert!(reject_sqlite("postgres://localhost/db").is_ok());
        assert!(reject_sqlite("postgresql://localhost/db").is_ok());
    }

    // ---- Override short-circuit + shutdown wiring (hermetic) -----------------

    /// A supervisor spy that records calls and never touches a real process.
    #[derive(Clone, Default)]
    struct SpySupervisor {
        provisioned: Arc<AtomicUsize>,
        started: Arc<AtomicUsize>,
        stopped: Arc<AtomicUsize>,
    }

    impl ChildPostgres for SpySupervisor {
        fn provision(&self) -> Result<(), OpenGeoError> {
            self.provisioned.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn start(&self) -> Result<String, OpenGeoError> {
            self.started.fetch_add(1, Ordering::SeqCst);
            Ok("postgres://managed@%2Ftmp/opengeo".into())
        }
        fn stop(&self) -> Result<(), OpenGeoError> {
            self.stopped.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn external_url_short_circuits_child() {
        let spy = SpySupervisor::default();
        let ds =
            resolve_datastore(Some("postgres://ext/db".into()), Box::new(spy.clone())).unwrap();
        assert_eq!(ds.database_url(), "postgres://ext/db");
        // The child was NEVER provisioned or started.
        assert_eq!(spy.provisioned.load(Ordering::SeqCst), 0);
        assert_eq!(spy.started.load(Ordering::SeqCst), 0);
        assert!(matches!(ds, Datastore::External(_)));
    }

    #[test]
    fn external_sqlite_url_is_rejected_before_any_child() {
        let spy = SpySupervisor::default();
        let result = resolve_datastore(Some("sqlite::memory:".into()), Box::new(spy.clone()));
        assert!(matches!(result, Err(OpenGeoError::Config(_))));
        assert_eq!(spy.provisioned.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn absent_url_provisions_and_starts_child() {
        let spy = SpySupervisor::default();
        let ds = resolve_datastore(None, Box::new(spy.clone())).unwrap();
        assert_eq!(spy.provisioned.load(Ordering::SeqCst), 1);
        assert_eq!(spy.started.load(Ordering::SeqCst), 1);
        assert_eq!(ds.database_url(), "postgres://managed@%2Ftmp/opengeo");
        assert!(matches!(ds, Datastore::Managed(_)));
    }

    #[test]
    fn dropping_managed_handle_stops_child() {
        let spy = SpySupervisor::default();
        let stopped = spy.stopped.clone();
        {
            let ds = resolve_datastore(None, Box::new(spy.clone())).unwrap();
            assert_eq!(stopped.load(Ordering::SeqCst), 0);
            drop(ds); // shutdown wiring: Drop stops the child exactly once.
        }
        assert_eq!(stopped.load(Ordering::SeqCst), 1);
    }

    // ---- Real-PG integration test (NETWORK/BINARY — ignored in CI) -----------

    /// Boots a REAL child Postgres by shelling out to `initdb`/`pg_ctl` on PATH.
    /// `#[ignore]`d so CI never downloads binaries or boots PG. Run explicitly:
    /// `cargo test -p opengeo-cli -- --ignored boots_real_child_postgres`.
    #[test]
    #[ignore = "boots a real child Postgres; requires initdb/pg_ctl on PATH"]
    fn boots_real_child_postgres() {
        let tmp = tempfile::tempdir().unwrap();
        let layout = DataLayout {
            root: tmp.path().to_path_buf(),
        };
        let sup = PgCliSupervisor::new(layout.clone());
        sup.provision().expect("provision");
        assert!(layout.is_initialised());
        let url = sup.start().expect("start");
        // Socket dir is the percent-encoded authority host, not a query param.
        assert!(url.starts_with("postgres://"));
        assert!(url.contains("%2F"));
        assert!(!url.contains(":5432"));
        sup.stop().expect("stop");
    }
}
