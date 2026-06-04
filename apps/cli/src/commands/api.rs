#![allow(clippy::doc_overindented_list_items)]
//! `ogeo api key …` — Phase 2 Story 12.1 API-key management.
//!
//! Subcommands:
//! - `create --name <slug>`   — generate and persist a fresh key, print the
//!                              plaintext to stdout exactly ONCE.
//! - `list`                   — print active and revoked keys (display
//!                              prefix only — the plaintext is unrecoverable).
//! - `revoke --name <slug>`   — soft-revoke by name; idempotent.

use clap::Args;
use opengeo_core::{api_key, Config, OpenGeoError};
use opengeo_storage::Storage;

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Slug-safe name for the key. Used as the display label and revoke
    /// target.
    #[arg(long)]
    pub name: String,
    /// Path to `opengeo.yaml`. Defaults to `./opengeo.yaml`.
    #[arg(long, default_value = "opengeo.yaml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long, default_value = "opengeo.yaml")]
    pub config: std::path::PathBuf,
    /// Hide revoked rows (default shows them for audit visibility).
    #[arg(long)]
    pub active_only: bool,
}

#[derive(Debug, Args)]
pub struct RevokeArgs {
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub reason: Option<String>,
    #[arg(long, default_value = "opengeo.yaml")]
    pub config: std::path::PathBuf,
}

pub async fn run_create(args: CreateArgs) -> Result<(), OpenGeoError> {
    let project_id = project_id_from_config(&args.config)?;
    let storage = connect_storage().await?;
    let key = api_key::generate();
    storage
        .api_keys()
        .insert(
            project_id,
            &args.name,
            &key.sha256_hash,
            &key.display_prefix,
        )
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;

    println!("Created API key `{}`.", args.name);
    println!();
    println!("    {}", key.plaintext);
    println!();
    println!("This value will not be shown again. Store it somewhere safe — usually");
    println!("export OPENGEO_API_KEY=… in your shell profile, or pass it as a Bearer");
    println!("header to the OpenGEO REST API.");
    Ok(())
}

pub async fn run_list(args: ListArgs) -> Result<(), OpenGeoError> {
    let project_id = project_id_from_config(&args.config)?;
    let storage = connect_storage().await?;
    let rows = storage
        .api_keys()
        .list_for_project(project_id)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;

    if rows.is_empty() {
        println!("(no API keys for this project)");
        return Ok(());
    }
    println!(
        "{:<24} {:<14} {:<22} {:<22} STATUS",
        "NAME", "PREFIX", "CREATED", "LAST USED"
    );
    for row in rows {
        if args.active_only && row.revoked_at.is_some() {
            continue;
        }
        let status = if let Some(ts) = row.revoked_at {
            format!("revoked {}", ts.format("%Y-%m-%d"))
        } else {
            "active".to_string()
        };
        let last_used = row
            .last_used_at
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".to_string());
        println!(
            "{:<24} ogeo_{:<9} {:<22} {:<22} {}",
            row.name,
            row.prefix,
            row.created_at.format("%Y-%m-%d %H:%M"),
            last_used,
            status,
        );
    }
    Ok(())
}

pub async fn run_revoke(args: RevokeArgs) -> Result<(), OpenGeoError> {
    let project_id = project_id_from_config(&args.config)?;
    let storage = connect_storage().await?;
    let revoked = storage
        .api_keys()
        .revoke(project_id, &args.name, args.reason.as_deref())
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;

    if revoked {
        println!("Revoked API key `{}`.", args.name);
    } else {
        println!(
            "No active API key named `{}` found (already revoked or never existed).",
            args.name
        );
    }
    Ok(())
}

fn project_id_from_config(path: &std::path::Path) -> Result<opengeo_core::ProjectId, OpenGeoError> {
    let yaml = std::fs::read_to_string(path)
        .map_err(|e| OpenGeoError::Config(format!("could not read {}: {e}", path.display())))?;
    let cfg = Config::from_yaml_str(&yaml)
        .map_err(|e| OpenGeoError::Config(format!("could not parse {}: {e}", path.display())))?;
    Ok(cfg.project_id())
}

async fn connect_storage() -> Result<Storage, OpenGeoError> {
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| OpenGeoError::Config("DATABASE_URL is required for `ogeo api key`".into()))?;
    let storage = Storage::connect(&url)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    // Run migrations so `ogeo api key create` on a fresh database does not
    // fail with "relation api_keys does not exist". `apps/api` also runs
    // migrations at boot; both paths use the same idempotent migrator.
    storage
        .migrate()
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    Ok(storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Wrap the api subcommands in a Parser so we can exercise the clap
    /// shape from tests. `ApiSub` lives in the lib's top-level enum but
    /// we don't want to depend on it from this test (would form a cycle).
    #[derive(Parser)]
    struct CreateProbe {
        #[command(flatten)]
        args: CreateArgs,
    }

    #[derive(Parser)]
    struct ListProbe {
        #[command(flatten)]
        args: ListArgs,
    }

    #[derive(Parser)]
    struct RevokeProbe {
        #[command(flatten)]
        args: RevokeArgs,
    }

    #[test]
    fn create_args_required_name_field_is_long_only() {
        let probe = CreateProbe::try_parse_from(["test", "--name", "ci-bot"]).unwrap();
        assert_eq!(probe.args.name, "ci-bot");
    }

    #[test]
    fn revoke_args_optional_reason_is_long_only() {
        let probe = RevokeProbe::try_parse_from([
            "test",
            "--name",
            "leaked-key",
            "--reason",
            "found in github gist",
        ])
        .unwrap();
        assert_eq!(probe.args.name, "leaked-key");
        assert_eq!(probe.args.reason.as_deref(), Some("found in github gist"));
    }

    #[test]
    fn list_args_default_active_only_is_false() {
        let probe = ListProbe::try_parse_from(["test"]).unwrap();
        assert!(!probe.args.active_only);
    }

    #[test]
    fn list_args_with_active_only_flag() {
        let probe = ListProbe::try_parse_from(["test", "--active-only"]).unwrap();
        assert!(probe.args.active_only);
    }
}
