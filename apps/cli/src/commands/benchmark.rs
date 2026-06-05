//! `ogeo benchmark optin|optout|status` — Phase 2 Story 13.1 CLI.
//!
//! Drives the opt-in/opt-out lifecycle for the public benchmark
//! contribution dataset. `pull` lives in a follow-up alongside the
//! out-of-process benchmark service (architecture §7); the OSS-side
//! state machine + consent record is what ships here.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anseo_benchmark::{ProjectKek, TERMS_VERSION};
use anseo_core::{OpenGeoError, ProjectId};
use anseo_storage::repositories::benchmark_consent::BenchmarkConsentRepo;
use anseo_storage::Storage;
use chrono::Utc;
use clap::Args;

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
    /// Skip the interactive confirmation. Because opt-out CRYPTO-SHREDS the
    /// project's benchmark key (an irreversible erasure), confirmation is
    /// required unless you pass this flag.
    #[arg(long)]
    pub yes: bool,
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
        io::stdin()
            .lock()
            .read_line(&mut answer)
            .map_err(|e| OpenGeoError::Config(format!("failed to read confirmation: {e}")))?;
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
    let project_str = project_id.to_string();

    // Opt-out is a TRUE opt-out (Story 39.2): it crypto-shreds the project's
    // benchmark KEK. Destroying that single key makes every wrapped DEK — and
    // therefore every benchmark contribution this project ever sealed —
    // permanently undecryptable. This is irreversible and is the mechanism by
    // which OpenGEO honours GDPR Art.17 ("right to erasure").
    print_shred_warning(&project_str);

    if !args.yes {
        print!(
            "To confirm this IRREVERSIBLE erasure, type the project id `{project_str}` exactly: "
        );
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin()
            .lock()
            .read_line(&mut answer)
            .map_err(|e| OpenGeoError::Config(format!("failed to read confirmation: {e}")))?;
        if answer.trim() != project_str {
            return Err(OpenGeoError::Config(
                "opt-out cancelled — the project id was not entered exactly. Nothing was \
                 destroyed; your benchmark key and contributions are untouched. (Re-run with \
                 `--yes` to skip this prompt.)"
                    .into(),
            ));
        }
    }

    // Record the consent event FIRST, so the audit trail captures the opt-out
    // intent even if the key destruction step surfaces a backend error.
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

    // CRYPTO-SHRED: destroy the per-project KEK across every SecretStore leg.
    // Idempotent — a project that never sealed a contribution simply has no
    // KEK to remove, and that is still a successful, complete opt-out.
    let store = anseo_core::default_chain();
    ProjectKek::destroy(&store, &project_str).map_err(|e| {
        OpenGeoError::Config(format!(
            "opt-out recorded (event id {id}) but crypto-shred of the benchmark key FAILED: {e}. \
             The contributions are NOT yet cryptographically erased. Resolve the secret-store \
             backend error and re-run `ogeo benchmark optout` to complete the erasure."
        ))
    })?;

    println!();
    println!(
        "✓ Opted out and CRYPTO-SHREDDED (event id {id}, terms version {TERMS_VERSION}, at {}).",
        Utc::now().to_rfc3339()
    );
    println!(
        "  The benchmark key for project `{project_str}` has been destroyed. Every contribution \
         this project sealed is now permanently undecryptable."
    );
    println!(
        "  This was an INTENTIONAL erasure — not an accidental key loss. There is no recovery: \
         re-opting in mints a brand-new key that cannot open any prior contribution."
    );
    Ok(())
}

/// Print the honest-scope warning before an irreversible crypto-shred opt-out.
///
/// Mirrors the honest-scope language from the compliance addendum: the
/// cryptographic guarantee holds only for media under OpenGEO's control;
/// operator-managed backups, snapshots and WAL are explicitly OUT OF SCOPE.
fn print_shred_warning(project_str: &str) {
    eprintln!("⚠  IRREVERSIBLE DESTRUCTIVE ACTION — benchmark opt-out crypto-shred");
    eprintln!();
    eprintln!(
        "  Opting out destroys the encryption key for project `{project_str}`, which makes EVERY"
    );
    eprintln!(
        "  benchmark contribution this project ever made permanently undecryptable. This cannot"
    );
    eprintln!("  be undone. This is an intentional erasure, distinct from an accidental key loss.");
    eprintln!();
    eprintln!("  SCOPE OF THE GUARANTEE — please read:");
    eprintln!(
        "    The crypto-shred guarantee holds ONLY for key material and data under OpenGEO's"
    );
    eprintln!(
        "    direct control (the local secret store). It does NOT reach operator-managed copies:"
    );
    eprintln!("      • backups of the secret store or database,");
    eprintln!("      • filesystem / volume snapshots,");
    eprintln!("      • database WAL / replication streams.");
    eprintln!("    Any such copy taken before this opt-out is OUT OF SCOPE for the cryptographic");
    eprintln!("    erasure and must be purged through your own backup-retention process.");
    eprintln!();
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
            println!(
                "Last event: {} ({})",
                row.event,
                row.created_at.to_rfc3339()
            );
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
    let path = config.unwrap_or(std::path::Path::new("anseo.yaml"));
    let cfg = anseo_core::Config::from_path(path).map_err(|e| {
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
