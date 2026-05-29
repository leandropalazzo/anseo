//! `opengeo-mcp` — MCP server binary (Story 16.1).
//!
//! Hand-rolled JSON-RPC 2.0 over stdio per OQ-P3-1 (Phase 3 kickoff
//! decisions). Calls the local OpenGEO `/v1` REST surface over loopback
//! HTTP per AD-Phase3-MCP-Process-Model (architecture-phase3-mcp-server.md
//! §5). Tool bodies land in Stories 16.2-16.5.

mod dispatch;
mod error;
mod http_client;
mod protocol;
mod tools;
mod transport;

use opengeo_core::telemetry::init_tracing;

use crate::dispatch::Dispatcher;
use crate::http_client::ApiClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `--version` exits before tracing init so it's safe to invoke without an
    // env. Driven by the [mcp-1] GA criterion.
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        if arg == "--version" || arg == "-V" {
            println!("opengeo-mcp {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
    }

    init_tracing("opengeo-mcp")?;

    let api_base = std::env::var("OPENGEO_API_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let api_key = std::env::var("OPENGEO_API_KEY").unwrap_or_default();
    let project =
        std::env::var("OPENGEO_PROJECT_ID").unwrap_or_else(|_| "default".to_string());

    tracing::info!(
        event = "service.boot",
        service = "opengeo-mcp",
        api_base = %api_base,
        project = %project,
        "opengeo-mcp booting (stdio transport)"
    );

    let api = ApiClient::new(api_base, api_key, project)?;
    let dispatcher = Dispatcher::new(api);

    transport::stdio::run(dispatcher).await?;

    tracing::info!(event = "service.shutdown", service = "opengeo-mcp", "stdio EOF");
    Ok(())
}
