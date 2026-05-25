//! `ogeo dashboard open` — FR-16.
//!
//! Opens the local Dashboard in the user's default browser. Phase 1 defers the
//! "start the web server if not running" half of the AC to Story 4.5 — when
//! `apps/web` becomes a real bundled server. For now we open the URL and rely
//! on `docker compose up` (Story 1.4) being the path that brings the web
//! container online.

use clap::Args;
use opengeo_core::OpenGeoError;

#[derive(Debug, Args)]
pub struct OpenArgs {
    /// Dashboard URL. Defaults to the OGEO_DASHBOARD_URL env var or
    /// `http://127.0.0.1:5173` (the compose default).
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// Print the URL to stdout instead of attempting to open a browser.
    /// Useful on headless hosts (CI, ssh-only servers).
    #[arg(long)]
    pub print: bool,
}

pub fn run(args: OpenArgs) -> Result<(), OpenGeoError> {
    let url = resolve_url(args.url);
    if args.print {
        println!("{url}");
        return Ok(());
    }
    open_in_browser(&url)?;
    eprintln!("Opening {url} ...");
    Ok(())
}

fn resolve_url(arg: Option<String>) -> String {
    if let Some(u) = arg {
        return u;
    }
    if let Ok(u) = std::env::var("OGEO_DASHBOARD_URL") {
        if !u.is_empty() {
            return u;
        }
    }
    "http://127.0.0.1:5173".to_string()
}

#[cfg(target_os = "macos")]
fn open_in_browser(url: &str) -> Result<(), OpenGeoError> {
    std::process::Command::new("open")
        .arg(url)
        .status()
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("failed to spawn `open`: {e}")))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn open_in_browser(url: &str) -> Result<(), OpenGeoError> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .status()
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("failed to spawn `xdg-open`: {e}")))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_in_browser(url: &str) -> Result<(), OpenGeoError> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status()
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("failed to spawn `start`: {e}")))?;
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn open_in_browser(_url: &str) -> Result<(), OpenGeoError> {
    Err(OpenGeoError::Internal(anyhow::anyhow!(
        "unsupported platform for `ogeo dashboard open`; use `--print` and open the URL yourself"
    )))
}
