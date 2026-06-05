//! `ogeo mcp {serve, status, install-config}` — Phase 3 Story 16.7.
//!
//! * `serve` — delegates to the `opengeo-mcp` binary.
//! * `status` — TCP-connect probe; informational only (exits 0).
//! * `install-config` — writes the mcpServers JSON snippet into the
//!   appropriate client config file.

use std::{net::TcpStream, path::PathBuf, time::Duration};

use anseo_core::OpenGeoError;
use clap::Args;

// ---------------------------------------------------------------------------
// serve
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Transport to use: "stdio" (default) or "http+sse".
    #[arg(long, default_value = "stdio")]
    pub transport: String,

    /// Bind address for http+sse transport.
    #[arg(long, default_value = "127.0.0.1:7071")]
    pub bind: String,

    /// Require API key for public HTTP+SSE access.
    #[arg(long)]
    pub allow_public: bool,
}

pub fn run_serve(args: ServeArgs) -> Result<(), OpenGeoError> {
    // Resolve the opengeo-mcp binary path.
    let bin = std::env::var("ANSEO_MCP_BIN")
        .or_else(|_| std::env::var("OPENGEO_MCP_BIN")) // deprecated alias
        .unwrap_or_else(|_| "anseo-mcp".to_string());

    let mut argv: Vec<String> = Vec::new();
    argv.push("--transport".into());
    argv.push(args.transport.clone());

    if args.transport == "http+sse" {
        argv.push("--bind".into());
        argv.push(args.bind.clone());
    }

    if args.allow_public {
        argv.push("--allow-public".into());
    }

    let status = std::process::Command::new(&bin)
        .args(&argv)
        .status()
        .map_err(|e| {
            OpenGeoError::Config(format!(
                "failed to launch {bin}: {e}. \
                 Ensure `anseo-mcp` is on your PATH or set ANSEO_MCP_BIN."
            ))
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(OpenGeoError::Config("anseo-mcp exited non-zero".into()))
    }
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// HTTP+SSE base URL to probe (defaults to http://127.0.0.1:7071).
    #[arg(long, default_value = "http://127.0.0.1:7071")]
    pub base_url: String,
}

/// Parse `host:port` from a bare URL like `http://127.0.0.1:7071`.
fn parse_addr(base_url: &str) -> Option<String> {
    // Strip scheme
    let without_scheme = base_url
        .strip_prefix("http://")
        .or_else(|| base_url.strip_prefix("https://"))
        .unwrap_or(base_url);

    // Drop any path component
    let host_port = without_scheme.split('/').next()?;
    Some(host_port.to_string())
}

pub fn run_status(args: StatusArgs) -> Result<(), OpenGeoError> {
    let addr = match parse_addr(&args.base_url) {
        Some(a) => a,
        None => {
            println!(
                "MCP server unreachable: could not parse address from {}",
                args.base_url
            );
            return Ok(());
        }
    };

    match TcpStream::connect_timeout(
        &addr
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:7071".parse().unwrap()),
        Duration::from_secs(3),
    ) {
        Ok(_) => println!("MCP server reachable at {}", args.base_url),
        Err(e) => println!("MCP server unreachable: {e}"),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// install-config
// ---------------------------------------------------------------------------

#[derive(Debug, Args)]
pub struct InstallConfigArgs {
    /// Target client: "claude-desktop" (default), "cursor", "zed".
    #[arg(default_value = "claude-desktop")]
    pub client: String,

    /// Override the config file path (for testing).
    #[arg(long)]
    pub config_path: Option<PathBuf>,

    /// API key to embed in the config snippet.
    #[arg(long, env = "ANSEO_API_KEY")]
    pub api_key: Option<String>,
}

fn home_dir() -> Result<PathBuf, OpenGeoError> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map_err(|_| OpenGeoError::Config("cannot determine home directory".into()))
}

fn default_config_path(client: &str) -> Result<PathBuf, OpenGeoError> {
    match client {
        "claude-desktop" => {
            #[cfg(target_os = "macos")]
            {
                Ok(home_dir()?
                    .join("Library/Application Support/Claude/claude_desktop_config.json"))
            }
            #[cfg(target_os = "windows")]
            {
                let appdata = std::env::var("APPDATA")
                    .map_err(|_| OpenGeoError::Config("%APPDATA% not set".into()))?;
                Ok(PathBuf::from(appdata)
                    .join("Claude")
                    .join("claude_desktop_config.json"))
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                Ok(home_dir()?.join(".config/Claude/claude_desktop_config.json"))
            }
        }
        "cursor" => Ok(home_dir()?.join(".cursor/mcp.json")),
        "zed" => Ok(home_dir()?.join(".config/zed/settings.json")),
        other => Err(OpenGeoError::Config(format!(
            "unknown client {other:?}. Supported: claude-desktop, cursor, zed"
        ))),
    }
}

pub fn run_install_config(args: InstallConfigArgs) -> Result<(), OpenGeoError> {
    // Validate client name regardless of whether config_path is overridden.
    let valid_clients = ["claude-desktop", "cursor", "zed"];
    if !valid_clients.contains(&args.client.as_str()) {
        return Err(OpenGeoError::Config(format!(
            "unknown client {:?}. Supported: claude-desktop, cursor, zed",
            args.client
        )));
    }

    let path = match args.config_path {
        Some(p) => p,
        None => default_config_path(&args.client)?,
    };

    let api_key = args
        .api_key
        .as_deref()
        .unwrap_or("YOUR_API_KEY_HERE")
        .to_string();

    let snippet = serde_json::json!({
        "mcpServers": {
            "anseo": {
                "command": "anseo-mcp",
                "env": {
                    "ANSEO_API_KEY": api_key,
                    "ANSEO_API_URL": "http://127.0.0.1:8080",
                    "ANSEO_PROJECT_ID": "default"
                }
            }
        }
    });

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            OpenGeoError::Config(format!(
                "failed to create config dir {}: {e}",
                parent.display()
            ))
        })?;
    }

    let json = serde_json::to_string_pretty(&snippet)
        .map_err(|e| OpenGeoError::Config(format!("JSON serialization error: {e}")))?;

    std::fs::write(&path, json)
        .map_err(|e| OpenGeoError::Config(format!("failed to write {}: {e}", path.display())))?;

    let label = match args.client.as_str() {
        "claude-desktop" => "Claude Desktop",
        "cursor" => "Cursor",
        "zed" => "Zed",
        other => other,
    };

    println!("Wrote {label} MCP config to: {}", path.display());
    Ok(())
}
