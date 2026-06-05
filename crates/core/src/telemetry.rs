//! Telemetry contract — structured-log field names, event names, and the
//! shared tracing initializer used by every Rust binary.
//!
//! All Prompt Run lifecycle events emit JSON to stdout through
//! `tracing-bunyan-formatter`. Field names are stable; renaming any of the
//! constants in [`fields`] is a breaking change.

use tracing::subscriber::SetGlobalDefaultError;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

/// Stable structured-log field names. NFR-Observability — these MUST appear
/// on every Prompt Run lifecycle event. Adding new fields is non-breaking;
/// renaming is breaking.
pub mod fields {
    pub const RUN_ID: &str = "run_id";
    pub const PROMPT_ID: &str = "prompt_id";
    pub const PROVIDER: &str = "provider";
    pub const MODEL: &str = "model";
    pub const EVENT: &str = "event";
    pub const DURATION_MS: &str = "duration_ms";
    pub const STATUS: &str = "status";
    /// Optional. Populated only on failure.
    pub const ERROR_KIND: &str = "error_kind";
    /// Per-request correlation ID (ULID). Threaded through every log line
    /// within a single API/MCP request.
    pub const REQUEST_ID: &str = "request_id";
}

/// Stable values for the `event` field. Reference only — emitters use the
/// constants directly so the wire-string can't drift.
pub mod events {
    pub const PROMPT_RUN_STARTED: &str = "prompt_run.started";
    pub const PROMPT_RUN_COMPLETED: &str = "prompt_run.completed";
    pub const PROMPT_RUN_FAILED: &str = "prompt_run.failed";
    pub const PROVIDER_REQUEST_STARTED: &str = "provider.request.started";
    pub const PROVIDER_REQUEST_COMPLETED: &str = "provider.request.completed";
    pub const CONFIG_LOADED: &str = "config.loaded";
}

/// Initialize the global tracing subscriber with a bunyan-formatted JSON layer.
///
/// Filter precedence: `ANSEO_LOG` env var > `OPENGEO_LOG` (deprecated) > `RUST_LOG` env var > `anseo=info`.
/// Output goes to stdout. Safe to call once per process; subsequent calls return
/// the underlying `SetGlobalDefaultError`.
pub fn init_tracing(service_name: &str) -> Result<(), SetGlobalDefaultError> {
    init_tracing_with_writer(service_name, std::io::stdout)
}

/// Like [`init_tracing`] but logs to **stderr** instead of stdout.
///
/// Required by the stdio MCP transport: stdout is the JSON-RPC protocol channel,
/// so any log frame written there corrupts the stream and prevents the client
/// from attaching. Use this in any binary that owns stdout for a wire protocol.
pub fn init_tracing_stderr(service_name: &str) -> Result<(), SetGlobalDefaultError> {
    init_tracing_with_writer(service_name, std::io::stderr)
}

/// Initialize the global tracing subscriber, sending bunyan JSON to `writer`.
fn init_tracing_with_writer<W>(service_name: &str, writer: W) -> Result<(), SetGlobalDefaultError>
where
    W: for<'a> tracing_subscriber::fmt::MakeWriter<'a> + Send + Sync + 'static,
{
    let env_filter = EnvFilter::try_from_env("ANSEO_LOG")
        .or_else(|_| EnvFilter::try_from_env("OPENGEO_LOG")) // deprecated alias
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("anseo=info"));

    let formatting_layer = BunyanFormattingLayer::new(service_name.to_owned(), writer);
    let subscriber = Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);

    tracing::subscriber::set_global_default(subscriber)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_constants_are_stable() {
        assert_eq!(fields::RUN_ID, "run_id");
        assert_eq!(fields::PROMPT_ID, "prompt_id");
        assert_eq!(fields::PROVIDER, "provider");
        assert_eq!(fields::MODEL, "model");
        assert_eq!(fields::EVENT, "event");
        assert_eq!(fields::DURATION_MS, "duration_ms");
        assert_eq!(fields::STATUS, "status");
        assert_eq!(fields::ERROR_KIND, "error_kind");
        assert_eq!(fields::REQUEST_ID, "request_id");
    }

    #[test]
    fn event_constants_are_stable() {
        assert_eq!(events::PROMPT_RUN_STARTED, "prompt_run.started");
        assert_eq!(events::PROMPT_RUN_COMPLETED, "prompt_run.completed");
        assert_eq!(events::PROMPT_RUN_FAILED, "prompt_run.failed");
        assert_eq!(events::PROVIDER_REQUEST_STARTED, "provider.request.started");
        assert_eq!(
            events::PROVIDER_REQUEST_COMPLETED,
            "provider.request.completed"
        );
        assert_eq!(events::CONFIG_LOADED, "config.loaded");
    }
}
