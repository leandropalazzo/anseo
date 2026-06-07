//! `ogeo db backup` / `ogeo db restore` — FR-21 / architecture I-7.
//!
//! Thin wrappers around `pg_dump` / `psql`. The CLI does not embed Postgres
//! itself; it expects the binaries to be on `$PATH`. `DATABASE_URL` is read
//! from the environment or from `--database-url`.

use std::path::PathBuf;

use anseo_core::OpenGeoError;
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum DbSub {
    /// Produce a portable `pg_dump` archive of the local Postgres instance.
    Backup(BackupArgs),
    /// Restore a backup written by `ogeo db backup`.
    Restore(RestoreArgs),
}

#[derive(Debug, Args)]
pub struct BackupArgs {
    /// Output path. Defaults to `anseo-backup-<ISO date>.sql.gz` in the
    /// current working directory.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Override `DATABASE_URL`.
    #[arg(long, value_name = "URL")]
    pub database_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct RestoreArgs {
    /// Backup file produced by `ogeo db backup`.
    pub input: PathBuf,

    /// Override `DATABASE_URL`.
    #[arg(long, value_name = "URL")]
    pub database_url: Option<String>,
}

pub fn run_backup(args: BackupArgs) -> Result<(), OpenGeoError> {
    let database_url = resolve_database_url(args.database_url.clone())?;
    let output = args.output.clone().unwrap_or_else(default_output_path);

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OpenGeoError::Internal(anyhow::anyhow!(
                    "failed to create output dir `{}`: {e}",
                    parent.display()
                ))
            })?;
        }
    }

    // pg_dump | gzip > output  (shell pipeline kept inside Rust so we can
    // detect both halves' exit status).
    let pg_dump = std::process::Command::new("pg_dump")
        .arg("--format=plain")
        .arg("--no-owner")
        .arg("--no-acl")
        .arg(&database_url)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| OpenGeoError::Data(format!("pg_dump not on PATH or failed to start: {e}")))?;

    let pg_stdout = pg_dump.stdout.expect("piped stdout");
    let mut gzip = std::process::Command::new("gzip")
        .arg("-c")
        .stdin(std::process::Stdio::from(pg_stdout))
        .stdout(std::process::Stdio::from(
            std::fs::File::create(&output).map_err(|e| {
                OpenGeoError::Internal(anyhow::anyhow!(
                    "failed to create `{}`: {e}",
                    output.display()
                ))
            })?,
        ))
        .spawn()
        .map_err(|e| OpenGeoError::Data(format!("gzip not on PATH or failed to start: {e}")))?;

    let status = gzip
        .wait()
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("waiting on gzip failed: {e}")))?;
    if !status.success() {
        return Err(OpenGeoError::Data(format!(
            "backup failed (gzip exit {})",
            status.code().unwrap_or(-1)
        )));
    }
    eprintln!("Wrote {}", output.display());
    Ok(())
}

pub fn run_restore(args: RestoreArgs) -> Result<(), OpenGeoError> {
    let database_url = resolve_database_url(args.database_url.clone())?;
    let input = std::fs::File::open(&args.input).map_err(|e| {
        OpenGeoError::Data(format!(
            "failed to open backup `{}`: {e}",
            args.input.display()
        ))
    })?;
    let gunzip = std::process::Command::new("gunzip")
        .arg("-c")
        .stdin(std::process::Stdio::from(input))
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| OpenGeoError::Data(format!("gunzip not on PATH or failed: {e}")))?;
    let psql_stdin = gunzip.stdout.expect("piped");
    let mut psql = std::process::Command::new("psql")
        .arg(&database_url)
        .stdin(std::process::Stdio::from(psql_stdin))
        .spawn()
        .map_err(|e| OpenGeoError::Data(format!("psql not on PATH or failed: {e}")))?;
    let status = psql
        .wait()
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("waiting on psql failed: {e}")))?;
    if !status.success() {
        return Err(OpenGeoError::Data(format!(
            "restore failed (psql exit {})",
            status.code().unwrap_or(-1)
        )));
    }
    eprintln!("Restored from {}", args.input.display());
    Ok(())
}

fn resolve_database_url(arg: Option<String>) -> Result<String, OpenGeoError> {
    if let Some(u) = arg {
        return Ok(u);
    }
    std::env::var("DATABASE_URL").map_err(|_| {
        OpenGeoError::Config(
            "DATABASE_URL not set; pass --database-url or export DATABASE_URL".into(),
        )
    })
}

fn default_output_path() -> PathBuf {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    PathBuf::from(format!("anseo-backup-{stamp}.sql.gz"))
}
