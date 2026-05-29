//! `ogeo benchmark optin|optout|status` — Phase 2 Story 13.1 CLI.
//!
//! Drives the opt-in/opt-out lifecycle for the public benchmark
//! contribution dataset. `pull` lives in a follow-up alongside the
//! out-of-process benchmark service (architecture §7); the OSS-side
//! state machine + consent record is what ships here.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use chrono::Utc;
use clap::Args;
use opengeo_benchmark::TERMS_VERSION;
use opengeo_core::{OpenGeoError, ProjectId};
use opengeo_storage::repositories::benchmark_consent::BenchmarkConsentRepo;
use opengeo_storage::Storage;

const TERMS_PATH: &str = "docs/benchmark-terms/v1-2026-05-28.md";

#[derive(Debug, Args)]
pub struct OptinArgs {
    /// Path to opengeo.yaml. Defaults to `./opengeo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    /// Skip the interactive terms prompt and assume confirmation.
    #[arg(long)]
    pub yes: bool,
    /// Operator-facing actor identifier recorded in the audit log.
    /// Defaults to `$USER` or `cli`.
    #[arg(long)]
    pub actor: Option<String>,
    /// Free-form note recorded alongside the opt-in event.
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct OptoutArgs {
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub actor: Option<String>,
    #[arg(long)]
    pub note: Option<String>,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

pub async fn run_optin(args: OptinArgs) -> Result<(), OpenGeoError> {
    let (storage, project_id) = open_storage(args.config.as_deref()).await?;
    let terms = std::fs::read_to_string(TERMS_PATH).map_err(|e| {
        OpenGeoError::Config(format!(
            "failed to read benchmark terms at `{TERMS_PATH}`: {e}"
        ))
    })?;

    println!("{terms}");
    println!();
    println!(
        "Terms version (pinned): {TERMS_VERSION}\n\
         Source: {TERMS_PATH}\n"
    );

    if !args.yes {
        print!("Type `yes` to opt in: ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer).map_err(|e| {
            OpenGeoError::Config(format!("failed to read confirmation: {e}"))
        })?;
        if answer.trim() != "yes" {
            return Err(OpenGeoError::Config(
                "opt-in cancelled — type `yes` exactly to confirm".into(),
            ));
        }
    }

    let actor = resolve_actor(args.actor);
    let id = BenchmarkConsentRepo::new(storage.pool())
        .record_optin(
            project_id,
            TERMS_VERSION,
            actor.as_deref(),
            args.note.as_deref(),
        )
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to record opt-in: {e}")))?;
    println!(
        "✓ Opted in (event id {id}, terms version {TERMS_VERSION}, at {})",
        Utc::now().to_rfc3339()
    );
    Ok(())
}

pub async fn run_optout(args: OptoutArgs) -> Result<(), OpenGeoError> {
    let (storage, project_id) = open_storage(args.config.as_deref()).await?;
    let actor = resolve_actor(args.actor);
    let id = BenchmarkConsentRepo::new(storage.pool())
        .record_optout(
            project_id,
            TERMS_VERSION,
            actor.as_deref(),
            args.note.as_deref(),
        )
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to record opt-out: {e}")))?;
    println!(
        "✓ Opted out (event id {id}, terms version {TERMS_VERSION}, at {})",
        Utc::now().to_rfc3339()
    );
    Ok(())
}

pub async fn run_status(args: StatusArgs) -> Result<(), OpenGeoError> {
    let (storage, project_id) = open_storage(args.config.as_deref()).await?;
    let latest = BenchmarkConsentRepo::new(storage.pool())
        .latest_for_project(project_id)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to read consent state: {e}")))?;
    match latest {
        None => {
            println!("Benchmark contribution: not opted in");
            println!("Current terms version: {TERMS_VERSION}");
        }
        Some(row) => {
            let active = row.event == "optin" && row.terms_version == TERMS_VERSION;
            println!(
                "Benchmark contribution: {}",
                if active { "active" } else { "inactive" }
            );
            println!("Last event: {} ({})", row.event, row.created_at.to_rfc3339());
            println!("Recorded terms version: {}", row.terms_version);
            println!("Current terms version: {TERMS_VERSION}");
            if let Some(actor) = row.actor {
                println!("Actor: {actor}");
            }
            if let Some(note) = row.note {
                println!("Note: {note}");
            }
            if row.event == "optin" && row.terms_version != TERMS_VERSION {
                println!(
                    "⚠ Recorded consent is on stale terms version. Re-run `ogeo benchmark optin` to refresh."
                );
            }
        }
    }
    Ok(())
}

fn resolve_actor(arg: Option<String>) -> Option<String> {
    arg.or_else(|| std::env::var("USER").ok())
        .or_else(|| Some("cli".to_string()))
}

async fn open_storage(
    config: Option<&std::path::Path>,
) -> Result<(Storage, ProjectId), OpenGeoError> {
    let database_url = std::env::var("DATABASE_URL").map_err(|_| {
        OpenGeoError::Config("DATABASE_URL must be set to record consent events".into())
    })?;
    let path = config.unwrap_or(std::path::Path::new("opengeo.yaml"));
    let cfg = opengeo_core::Config::from_path(path).map_err(|e| {
        OpenGeoError::Config(format!(
            "failed to read project config at `{}`: {e}",
            path.display()
        ))
    })?;
    let storage = Storage::connect(&database_url)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to connect to Postgres: {e}")))?;
    Ok((storage, cfg.project_id()))
}
