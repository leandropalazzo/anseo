//! `opengeo-mcp` — MCP server binary (Story 16.1).
//!
//! Hand-rolled JSON-RPC 2.0 over stdio per OQ-P3-1 (Phase 3 kickoff
//! decisions). Calls the local OpenGEO `/v1` REST surface over loopback
//! HTTP per AD-Phase3-MCP-Process-Model (architecture-phase3-mcp-server.md
//! §5). Tool bodies land in Stories 16.2-16.5. HTTP+SSE transport added in
//! Story 16.6 per AD-Phase3-MCP-TransportDefault.

use std::net::SocketAddr;
use std::sync::Arc;

use opengeo_core::telemetry::init_tracing_stderr;
use opengeo_mcp::dispatch::Dispatcher;
use opengeo_mcp::http_client::ApiClient;
use opengeo_mcp::transport;

// ---------------------------------------------------------------------------
// CLI parsing (hand-rolled to avoid a clap dep in this lean binary).
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Transport {
    Stdio,
    HttpSse,
}

#[derive(Debug)]
struct CliArgs {
    transport: Transport,
    bind: SocketAddr,
    allow_public: bool,
    /// Story 36.5: per-session project override for the stdio transport.
    /// When set, overrides `OPENGEO_PROJECT_ID` for this server process.
    /// Per-call `tools/call` `project` fields still take precedence over this.
    project_override: Option<String>,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            transport: Transport::Stdio,
            bind: "127.0.0.1:7071".parse().expect("hard-coded default addr"),
            allow_public: false,
            project_override: None,
        }
    }
}

fn parse_args() -> Result<CliArgs, String> {
    let mut args = std::env::args().skip(1).peekable();
    let mut cli = CliArgs::default();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("opengeo-mcp {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--transport" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--transport requires a value".to_string())?;
                cli.transport = match val.as_str() {
                    "stdio" => Transport::Stdio,
                    "http+sse" => Transport::HttpSse,
                    other => {
                        return Err(format!(
                            "unknown transport '{other}'; expected 'stdio' or 'http+sse'"
                        ))
                    }
                };
            }
            "--bind" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--bind requires a value".to_string())?;
                cli.bind = val
                    .parse::<SocketAddr>()
                    .map_err(|e| format!("invalid --bind address '{val}': {e}"))?;
            }
            "--allow-public" => {
                cli.allow_public = true;
            }
            // Story 36.5: session-level project selector for stdio transport.
            "--project" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--project requires a value".to_string())?;
                if val.is_empty() {
                    return Err("--project value must not be empty".to_string());
                }
                cli.project_override = Some(val);
            }
            other => {
                return Err(format!("unknown argument '{other}'"));
            }
        }
    }

    Ok(cli)
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse CLI before tracing init so `--version` exits cleanly without any
    // env requirements (GA criterion mcp-1).
    let cli = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    // stdio transport owns stdout for the JSON-RPC protocol channel, so logs
    // MUST go to stderr — otherwise log frames corrupt the stream and the MCP
    // client cannot attach. (See transport/stdio.rs.)
    init_tracing_stderr("opengeo-mcp")?;

    let api_base =
        std::env::var("OPENGEO_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    let api_key = std::env::var("OPENGEO_API_KEY").unwrap_or_default();
    // Story 36.5: --project flag overrides OPENGEO_PROJECT_ID for this session.
    let project = cli.project_override.clone().unwrap_or_else(|| {
        std::env::var("OPENGEO_PROJECT_ID").unwrap_or_else(|_| "default".to_string())
    });

    // GA criterion mcp-9: --allow-public without an API key is refused.
    if cli.allow_public && api_key.is_empty() {
        eprintln!("Error: --allow-public requires OPENGEO_API_KEY to be set");
        std::process::exit(1);
    }

    match cli.transport {
        Transport::Stdio => {
            tracing::info!(
                event = "service.boot",
                service = "opengeo-mcp",
                transport = "stdio",
                api_base = %api_base,
                project = %project,
                "opengeo-mcp booting (stdio transport)"
            );

            let api = ApiClient::new(api_base, api_key, project)?;
            let dispatcher = Dispatcher::new(api);

            transport::stdio::run(dispatcher).await?;

            tracing::info!(
                event = "service.shutdown",
                service = "opengeo-mcp",
                "stdio EOF"
            );
        }

        Transport::HttpSse => {
            tracing::info!(
                event = "service.boot",
                service = "opengeo-mcp",
                transport = "http+sse",
                bind = %cli.bind,
                allow_public = cli.allow_public,
                api_base = %api_base,
                project = %project,
                "opengeo-mcp booting (HTTP+SSE transport)"
            );

            let api = ApiClient::new(api_base, api_key.clone(), project)?;
            let dispatcher = Arc::new(Dispatcher::new(api));

            transport::http::run(dispatcher, cli.bind, cli.allow_public, api_key).await?;

            tracing::info!(
                event = "service.shutdown",
                service = "opengeo-mcp",
                "HTTP+SSE listener closed"
            );
        }
    }

    Ok(())
}
