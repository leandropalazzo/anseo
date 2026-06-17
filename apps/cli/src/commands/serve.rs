//! `anseo serve` — single-binary supervisor (Story 37.1).
//!
//! Boots the HTTP API (`anseo-api`) and runs the background worker fan-out
//! loop (`anseo-worker`) IN-PROCESS in one binary, sharing one Postgres pool's
//! worth of connections, one shutdown signal, and one process lifetime.
//!
//! Supervision model:
//! - The worker runs as a SUPERVISED tokio task. The worker's own fan-out
//!   already isolates per-project faults at the join boundary
//!   (`anseo_worker::dispatch::run_isolated`); on top of that we run the whole
//!   loop inside its own task so that even a panic in the loop scaffolding does
//!   NOT abort the API task (tokio tasks are independent — one task's panic does
//!   not unwind another).
//! - SIGINT/SIGTERM trigger a graceful shutdown that stops BOTH: the signal
//!   fires a shared `watch` channel; the API uses it for axum graceful shutdown
//!   and the worker loop selects on it to break out of its poll loop.
//!
//! Datastore (37.4): when `DATABASE_URL` is set the external Postgres is used
//! unchanged; when it is absent a managed child Postgres is provisioned and
//! supervised for the process lifetime (see [`crate::datastore`]). The handle is
//! held in this function's scope so the child is stopped on shutdown.

use std::sync::Arc;

use anseo_api::boot::{build_api, serve_with_shutdown, ApiBootConfig};
use anseo_api::routes::serve_status::ServeInfo;
use anseo_core::OpenGeoError;
use anseo_storage::Storage;
use anseo_worker::run::{load_dispatch_context, run_poll_loop};
use clap::Args;

use crate::datastore::{resolve_from_env, Datastore};

/// Default port when neither `--port` nor a port in `--bind` is given.
const DEFAULT_PORT: u16 = 8080;
/// Localhost bind by default — security baseline (full gating is Story 37.7).
const DEFAULT_HOST: &str = "127.0.0.1";

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Directory holding project config (looks for `anseo.yaml` inside).
    /// Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    pub projects_dir: Option<std::path::PathBuf>,

    /// Full bind address (host:port). Overrides `--port`. Defaults to
    /// `127.0.0.1:<port>`. Bind to a non-loopback address only behind your own
    /// auth/network controls — a public bind with no API keys is refused.
    #[arg(long, value_name = "ADDR")]
    pub bind: Option<String>,

    /// Port to bind on `127.0.0.1` when `--bind` is not given.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
}

/// Returns `true` when the host part of `addr` is a loopback address
/// (IPv4 `127.x.x.x` family or IPv6 `::1`).
fn is_loopback_bind(addr: &str) -> bool {
    // Strip the port: take everything before the last `:`.
    let host = match addr.rfind(':') {
        Some(pos) => &addr[..pos],
        None => addr,
    };
    // Strip surrounding brackets for bare IPv6 literals like `[::1]:port`.
    let host = host
        .trim_matches(|c| c == '[' || c == ']')
        .trim_end_matches('.')
        .to_ascii_lowercase();
    // Parse as a std IP first; fall back to string prefix checks for the
    // unusual case where the host was specified as a bare hostname.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    // Conservative: unknown hostnames are treated as non-loopback.
    host == "localhost"
}

fn should_warn_public_bind(addr: &str) -> bool {
    !is_loopback_bind(addr)
}

fn public_bind_warning_message(addr: &str) -> String {
    format!(
        "\n\
⚠️  WARNING: binding to {addr} exposes Anseo on a non-loopback interface.\n\
   Anseo OSS has no built-in auth for the web or MCP surfaces.\n\
   Ensure a reverse proxy (Caddy, nginx) with TLS and auth is in front.\n\
   See docs/production-deployment.md for copy-paste Caddy/nginx configs.\n\n"
    )
}

/// Emit a startup warning when the operator binds to a non-loopback interface.
///
/// The OSS stack has no built-in authentication for the web or MCP surfaces.
/// Exposing those surfaces directly on a public interface without a reverse
/// proxy, TLS, and auth in front is a misconfiguration.  This is non-blocking
/// (a warning, not an error) to allow operators who know what they are doing
/// (e.g. behind a VPN or in a firewalled private network) to continue.
fn warn_if_public_bind(addr: &str) {
    if should_warn_public_bind(addr) {
        print!("{}", public_bind_warning_message(addr));
        tracing::warn!(
            event = "serve.public_bind",
            bind = addr,
            "non-loopback bind detected — ensure a reverse proxy + TLS + auth are in front"
        );
    }
}

/// Resolve the effective bind address from the flags.
fn resolve_bind(bind: &Option<String>, port: u16) -> String {
    match bind {
        Some(addr) => addr.clone(),
        None => format!("{DEFAULT_HOST}:{port}"),
    }
}

/// Resolve the `anseo.yaml` path from `--projects-dir`.
fn resolve_config_path(projects_dir: &Option<std::path::PathBuf>) -> String {
    match projects_dir {
        Some(dir) => dir.join("anseo.yaml").to_string_lossy().into_owned(),
        None => "anseo.yaml".to_string(),
    }
}

pub async fn run(args: ServeArgs) -> Result<(), OpenGeoError> {
    let bind_addr = resolve_bind(&args.bind, args.port);
    let config_path = resolve_config_path(&args.projects_dir);

    // Security baseline (Story 37.16 / RISK-5): warn early when the operator
    // binds to a non-loopback address without an obvious proxy in front.
    warn_if_public_bind(&bind_addr);

    // Resolve the datastore: external `DATABASE_URL` if set (behavior unchanged),
    // otherwise provision + start a managed child Postgres. `_datastore` is held
    // for the whole `serve` lifetime; dropping it on return stops the child.
    let datastore = resolve_from_env()?;
    if matches!(datastore, Datastore::Managed(_)) {
        println!("anseo serve — no DATABASE_URL; using the managed child Postgres datastore");
        tracing::info!(
            event = "serve.datastore_managed",
            "provisioned a managed child Postgres (no DATABASE_URL set)"
        );
    }
    let database_url = datastore.database_url().to_string();
    let _datastore = datastore;

    // Shared graceful-shutdown signal: one source, two consumers (API + worker).
    let (shutdown_tx, _) = tokio::sync::watch::channel(false);

    // Supervisor metadata: stamped at boot, injected into the API state so
    // `GET /v1/serve/status` can report the active tier and component liveness.
    let serve_info = Arc::new(ServeInfo::new());

    // Boot the API: connect storage, migrate, seed, run the bind guard, build
    // the router. This also spawns the NOTIFY→broadcast bridge.
    let booted = build_api(ApiBootConfig {
        database_url: database_url.clone(),
        bind_addr,
        config_path: config_path.clone(),
        serve_info: Some(serve_info),
    })
    .await
    .map_err(|e| OpenGeoError::Config(format!("failed to boot API: {e}")))?;

    let socket = booted.socket;

    // Print the bound URLs on startup (Task 4).
    println!("anseo serve — Anseo running in a single process");
    println!("  API:        http://{socket}");
    println!("  Health:     http://{socket}/healthz");
    println!("  Dashboard:  http://{socket}/");
    println!("Press Ctrl-C to stop.");
    tracing::info!(event = "serve.boot", bind = %socket, "anseo serve listening");

    // Worker substrate: a second Storage handle over its own pool (the API owns
    // its pool inside `AppState`). External `DATABASE_URL` for this story.
    let worker_storage = Storage::connect(&database_url)
        .await
        .map_err(|e| OpenGeoError::Config(format!("worker failed to connect to Postgres: {e}")))?;
    let dispatch = load_dispatch_context(&config_path);
    if dispatch.is_none() {
        tracing::warn!(
            event = "serve.dispatch_disabled",
            "no readable config or provider registry; schedule dispatch is inert (reaper + webhooks + ETL still run)"
        );
    }

    // SUPERVISED worker task. Running the loop in its own task means a panic in
    // the loop scaffolding is contained at THIS join boundary and cannot unwind
    // the API task. Per-project sweep faults are already isolated inside the
    // fan-out itself.
    let worker_shutdown = subscribe_shutdown(&shutdown_tx);
    let worker_handle = tokio::spawn(async move {
        run_poll_loop(&worker_storage, dispatch.as_ref(), worker_shutdown).await
    });

    // Serve the API with graceful shutdown driven by the shared signal. axum is
    // confined to the `anseo-api` crate behind `serve_with_shutdown`.
    let api_shutdown = subscribe_shutdown(&shutdown_tx);
    let server = serve_with_shutdown(booted, api_shutdown);

    // Translate OS signals into the shared shutdown broadcast.
    let signal_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        wait_for_signal().await;
        tracing::info!(
            event = "serve.shutdown",
            "shutdown signal received; stopping API + worker"
        );
        let _ = signal_tx.send(true);
    });

    // Run the API to completion. On graceful shutdown it returns; we then join
    // the worker so its in-flight tick drains before we exit.
    if let Err(err) = server.await {
        // Server error: trip shutdown so the worker also stops, then surface it.
        let _ = shutdown_tx.send(true);
        let _ = worker_handle.await;
        return Err(OpenGeoError::Config(format!("API server error: {err}")));
    }

    // API stopped (graceful). Ensure the worker is also told to stop, then join.
    let _ = shutdown_tx.send(true);
    match worker_handle.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::warn!(event = "serve.worker_error", error = %err, "worker loop returned an error on shutdown");
        }
        Err(join_err) => {
            // Contained: the worker task panicked, but the API already served
            // its lifetime — we log rather than crash the clean shutdown path.
            tracing::error!(event = "serve.worker_panicked", panic = %join_err, "worker task panicked; API was unaffected");
        }
    }

    tracing::info!(event = "serve.stopped", "anseo serve stopped cleanly");
    Ok(())
}

/// A future that resolves when the shared shutdown signal flips to `true`.
fn subscribe_shutdown(
    tx: &tokio::sync::watch::Sender<bool>,
) -> impl std::future::Future<Output = ()> + Send + 'static {
    let mut rx = tx.subscribe();
    async move {
        // Already-shutdown is fine (resolves immediately); otherwise wait for
        // the flip. A dropped sender also resolves us (changed() errors) so we
        // never hang on a closed channel.
        while !*rx.borrow() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }
}

/// Resolve on SIGINT (Ctrl-C) or, on Unix, SIGTERM.
async fn wait_for_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_loopback_bind ──────────────────────────────────────────────────────

    #[test]
    fn loopback_ipv4_is_detected() {
        assert!(is_loopback_bind("127.0.0.1:8080"));
        assert!(is_loopback_bind("127.0.0.1:0"));
    }

    #[test]
    fn loopback_ipv4_family_is_detected() {
        // The full 127/8 block is loopback per RFC 5735.
        assert!(is_loopback_bind("127.1.2.3:8080"));
    }

    #[test]
    fn loopback_ipv6_is_detected() {
        assert!(is_loopback_bind("[::1]:8080"));
    }

    #[test]
    fn localhost_hostname_is_loopback() {
        assert!(is_loopback_bind("localhost:8080"));
        assert!(is_loopback_bind("LOCALHOST:8080"));
        assert!(is_loopback_bind("localhost.:8080"));
    }

    #[test]
    fn public_ipv4_is_not_loopback() {
        assert!(!is_loopback_bind("0.0.0.0:8080"));
        assert!(!is_loopback_bind("192.168.1.1:8080"));
        assert!(!is_loopback_bind("10.0.0.1:8080"));
    }

    #[test]
    fn public_ipv6_is_not_loopback() {
        assert!(!is_loopback_bind("[::]:8080"));
    }

    #[test]
    fn public_bind_warning_decision_matches_security_baseline() {
        assert!(!should_warn_public_bind("127.0.0.1:8080"));
        assert!(!should_warn_public_bind("localhost:8080"));
        assert!(!should_warn_public_bind("LOCALHOST.:8080"));
        assert!(!should_warn_public_bind("[::1]:8080"));
        assert!(should_warn_public_bind("0.0.0.0:8080"));
        assert!(should_warn_public_bind("192.168.1.10:8080"));
        assert!(should_warn_public_bind("[::]:8080"));
    }

    #[test]
    fn public_bind_warning_message_matches_security_baseline() {
        let message = public_bind_warning_message("0.0.0.0:8080");

        assert!(message.contains(
            "WARNING: binding to 0.0.0.0:8080 exposes Anseo on a non-loopback interface."
        ));
        assert!(message.contains("no built-in auth for the web or MCP surfaces"));
        assert!(message.contains("reverse proxy (Caddy, nginx) with TLS and auth"));
        assert!(message.contains("docs/production-deployment.md"));
    }

    // ── resolve_bind ─────────────────────────────────────────────────────────

    #[test]
    fn resolve_bind_defaults_to_loopback() {
        assert_eq!(resolve_bind(&None, 8080), "127.0.0.1:8080");
        assert_eq!(resolve_bind(&None, 9999), "127.0.0.1:9999");
    }

    #[test]
    fn explicit_bind_overrides_port() {
        assert_eq!(
            resolve_bind(&Some("0.0.0.0:3000".into()), 8080),
            "0.0.0.0:3000"
        );
    }

    #[test]
    fn config_path_uses_projects_dir() {
        assert_eq!(resolve_config_path(&None), "anseo.yaml");
        let dir = std::path::PathBuf::from("/srv/proj");
        assert_eq!(resolve_config_path(&Some(dir)), "/srv/proj/anseo.yaml");
    }

    /// A flipped shutdown signal resolves the subscriber future immediately —
    /// the mechanism that stops both the API and the worker.
    #[tokio::test]
    async fn shutdown_signal_resolves_subscribers() {
        let (tx, _) = tokio::sync::watch::channel(false);
        let fut = subscribe_shutdown(&tx);
        tx.send(true).unwrap();
        // Must complete promptly; a 1s timeout guards against a hang.
        tokio::time::timeout(std::time::Duration::from_secs(1), fut)
            .await
            .expect("subscriber should resolve once shutdown is signalled");
    }
}
