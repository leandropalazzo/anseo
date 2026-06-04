//! `ogeo serve` — single-binary supervisor (Story 37.1).
//!
//! Boots the HTTP API (`opengeo-api`) and runs the background worker fan-out
//! loop (`opengeo-worker`) IN-PROCESS in one binary, sharing one Postgres pool's
//! worth of connections, one shutdown signal, and one process lifetime.
//!
//! Supervision model:
//! - The worker runs as a SUPERVISED tokio task. The worker's own fan-out
//!   already isolates per-project faults at the join boundary
//!   (`opengeo_worker::dispatch::run_isolated`); on top of that we run the whole
//!   loop inside its own task so that even a panic in the loop scaffolding does
//!   NOT abort the API task (tokio tasks are independent — one task's panic does
//!   not unwind another).
//! - SIGINT/SIGTERM trigger a graceful shutdown that stops BOTH: the signal
//!   fires a shared `watch` channel; the API uses it for axum graceful shutdown
//!   and the worker loop selects on it to break out of its poll loop.
//!
//! Web embedding (37.2), MCP fold (37.3), and managed child Postgres (37.4) are
//! later stories; here the datastore is an external `DATABASE_URL`.

use clap::Args;
use opengeo_api::boot::{build_api, serve_with_shutdown, ApiBootConfig};
use opengeo_core::OpenGeoError;
use opengeo_storage::Storage;
use opengeo_worker::run::{load_dispatch_context, run_poll_loop};

/// Default port when neither `--port` nor a port in `--bind` is given.
const DEFAULT_PORT: u16 = 8080;
/// Localhost bind by default — security baseline (full gating is Story 37.7).
const DEFAULT_HOST: &str = "127.0.0.1";

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Directory holding project config (looks for `opengeo.yaml` inside).
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

/// Resolve the effective bind address from the flags.
fn resolve_bind(bind: &Option<String>, port: u16) -> String {
    match bind {
        Some(addr) => addr.clone(),
        None => format!("{DEFAULT_HOST}:{port}"),
    }
}

/// Resolve the `opengeo.yaml` path from `--projects-dir`.
fn resolve_config_path(projects_dir: &Option<std::path::PathBuf>) -> String {
    match projects_dir {
        Some(dir) => dir.join("opengeo.yaml").to_string_lossy().into_owned(),
        None => "opengeo.yaml".to_string(),
    }
}

pub async fn run(args: ServeArgs) -> Result<(), OpenGeoError> {
    let database_url = std::env::var("DATABASE_URL").map_err(|_| {
        OpenGeoError::Config(
            "DATABASE_URL must be set for `ogeo serve` (external Postgres; managed datastore is Story 37.4)".into(),
        )
    })?;
    let bind_addr = resolve_bind(&args.bind, args.port);
    let config_path = resolve_config_path(&args.projects_dir);

    // Shared graceful-shutdown signal: one source, two consumers (API + worker).
    let (shutdown_tx, _) = tokio::sync::watch::channel(false);

    // Boot the API: connect storage, migrate, seed, run the bind guard, build
    // the router. This also spawns the NOTIFY→broadcast bridge.
    let booted = build_api(ApiBootConfig {
        database_url: database_url.clone(),
        bind_addr,
        config_path: config_path.clone(),
    })
    .await
    .map_err(|e| OpenGeoError::Config(format!("failed to boot API: {e}")))?;

    let socket = booted.socket;

    // Print the bound URLs on startup (Task 4).
    println!("ogeo serve — OpenGEO running in a single process");
    println!("  API:        http://{socket}");
    println!("  Health:     http://{socket}/healthz");
    println!("  Dashboard:  http://{socket}/");
    println!("Press Ctrl-C to stop.");
    tracing::info!(event = "serve.boot", bind = %socket, "ogeo serve listening");

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
    // confined to the `opengeo-api` crate behind `serve_with_shutdown`.
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

    tracing::info!(event = "serve.stopped", "ogeo serve stopped cleanly");
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
        assert_eq!(resolve_config_path(&None), "opengeo.yaml");
        let dir = std::path::PathBuf::from("/srv/proj");
        assert_eq!(resolve_config_path(&Some(dir)), "/srv/proj/opengeo.yaml");
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
