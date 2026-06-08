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
use anseo_storage::repositories::benchmark_consent::{BenchmarkConsentRepo, ConsentTier};
use anseo_storage::repositories::entities::EntityRepo;
use anseo_storage::Storage;
use chrono::Utc;
use clap::Args;

const TERMS_PATH: &str = "docs/benchmark-terms/v1-2026-05-28.md";

#[derive(Debug, Args)]
pub struct OptinArgs {
    /// Path to anseo.yaml. Defaults to `./anseo.yaml`.
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
    /// Opt into the BRAND-VISIBILITY (identified) tier instead of the anonymous
    /// aggregate tier (Story 44.1). This is the explicit, separately-revocable
    /// opt-in that lets your brand appear named and ranked in the public
    /// visibility leaderboard. Requires a verified domain. APPEARING ≠ CLAIMING.
    #[arg(long = "brand-visibility")]
    pub brand_visibility: bool,
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
    /// Withdraw from the BRAND-VISIBILITY (identified) tier only (Story 44.1).
    /// Single command, single confirmation (GDPR Art.7(3)). Future identified
    /// contributions stop immediately. This does NOT crypto-shred the benchmark
    /// key — anonymous aggregate contribution (if active) is unaffected. Omit
    /// this flag to perform the full anonymous-tier crypto-shred opt-out.
    #[arg(long = "brand-visibility")]
    pub brand_visibility: bool,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,
}

pub async fn run_optin(args: OptinArgs) -> Result<(), OpenGeoError> {
    if args.brand_visibility {
        return run_optin_brand_visibility(args).await;
    }

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

/// `ogeo benchmark optin --brand-visibility` (Story 44.1).
///
/// The identified tier is stricter than anonymous: it requires the anonymous
/// aggregate tier to already be active (you cannot be identified without first
/// contributing the aggregate signal) and shows exactly what ADDITIONAL data is
/// transmitted (the verification token + domain association — never a raw brand
/// name). Records a separate `tier = brand_visibility` consent row.
async fn run_optin_brand_visibility(args: OptinArgs) -> Result<(), OpenGeoError> {
    let (storage, project_id) = open_storage(args.config.as_deref()).await?;
    let repo = BenchmarkConsentRepo::new(storage.pool());

    // AC1: the identified tier builds ON TOP of the anonymous tier. Refuse if
    // the project is not currently anonymously opted in on the current terms.
    let anon = repo
        .latest_for_tier(project_id, ConsentTier::Anonymous)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to read anonymous consent: {e}")))?;
    let anon_active = anon.map(|r| r.is_active(TERMS_VERSION)).unwrap_or(false);
    if !anon_active {
        return Err(OpenGeoError::Config(
            "brand-visibility opt-in requires the anonymous aggregate tier to be active first. \
             Run `ogeo benchmark optin` (without --brand-visibility) and ensure your domain is \
             verified, then retry."
                .into(),
        ));
    }

    // SECURITY (44.1): the identified/named tier is a CLAIM, and the terms +
    // Tier-B contract require a DOMAIN-VERIFIED claim (Story 43.2). Hard-gate on
    // the project's domain being `verified` in the entity registry before we
    // record the consent — appearing in aggregate data is not the same as being
    // entitled to claim a named identity. Resolve the domain from the project
    // config's brand.site_url, normalize it the same way the registry keys do,
    // and require claim_status == "verified".
    let domain = project_domain(args.config.as_deref())?;
    let entity = EntityRepo::new(storage.pool())
        .get(&domain)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to read entity registry: {e}")))?;
    let verified = entity
        .as_ref()
        .map(|e| e.claim_status == "verified")
        .unwrap_or(false);
    if !verified {
        return Err(OpenGeoError::Config(format!(
            "brand-visibility requires a domain-verified claim for `{domain}`, but it is not \
             verified (status: {}). Verify ownership of your domain (DNS-TXT or email magic-link) \
             before opting into the named/ranked leaderboard, then retry.",
            entity
                .as_ref()
                .map(|e| e.claim_status.as_str())
                .unwrap_or("no claim"),
        )));
    }

    print_brand_visibility_terms();

    if !args.yes {
        print!("Type `yes` to opt into the brand-visibility (identified) tier: ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin()
            .lock()
            .read_line(&mut answer)
            .map_err(|e| OpenGeoError::Config(format!("failed to read confirmation: {e}")))?;
        if answer.trim() != "yes" {
            return Err(OpenGeoError::Config(
                "brand-visibility opt-in cancelled — type `yes` exactly to confirm".into(),
            ));
        }
    }

    let actor = resolve_actor(args.actor);
    let id = repo
        .record_optin_tier(
            project_id,
            ConsentTier::BrandVisibility,
            TERMS_VERSION,
            actor.as_deref(),
            args.note.as_deref(),
        )
        .await
        .map_err(|e| {
            OpenGeoError::Config(format!("failed to record brand-visibility opt-in: {e}"))
        })?;
    println!(
        "✓ Opted into brand-visibility (identified) tier (event id {id}, terms version \
         {TERMS_VERSION}, at {}).",
        Utc::now().to_rfc3339()
    );
    println!(
        "  Your brand will appear named and ranked in the public visibility leaderboard once \
         server-side resolution (44.2/44.3) is live. Withdraw any time with \
         `ogeo benchmark optout --brand-visibility`."
    );
    Ok(())
}

/// Print the stricter brand-visibility consent terms, spelling out exactly what
/// ADDITIONAL data the identified tier transmits beyond the anonymous tier.
fn print_brand_visibility_terms() {
    println!("BRAND-VISIBILITY (IDENTIFIED) TIER — additional consent");
    println!();
    println!("  This tier is STRICTER than the anonymous aggregate tier you already accepted. By");
    println!("  opting in you authorize the following ADDITIONAL data on the contribution path:");
    println!();
    println!("    • your verified-domain verification token (resolves to your brand identity");
    println!("      server-side via the entity registry — your brand NAME is never transmitted");
    println!("      by the client),");
    println!("    • the association between that token and this project's contributions.");
    println!();
    println!("  Effect: your brand appears NAMED and RANKED in the public visibility leaderboard.");
    println!("  APPEARING in aggregate data is not the same as CLAIMING a named identity — this");
    println!("  opt-in is what makes the claim. It is separate from, and revocable independently");
    println!("  of, your anonymous aggregate consent.");
    println!();
    println!("  Withdrawal is one action: `ogeo benchmark optout --brand-visibility` stops all");
    println!("  future identified contributions immediately (GDPR Art.7(3)).");
    println!();
    println!("  Terms version (pinned): {TERMS_VERSION}");
    println!();
}

pub async fn run_optout(args: OptoutArgs) -> Result<(), OpenGeoError> {
    if args.brand_visibility {
        return run_optout_brand_visibility(args).await;
    }

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
    let repo = BenchmarkConsentRepo::new(storage.pool());
    let id = repo
        .record_optout(
            project_id,
            TERMS_VERSION,
            actor.as_deref(),
            args.note.as_deref(),
        )
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to record opt-out: {e}")))?;

    // Story 44.1 autoreview fix: a FULL opt-out crypto-shreds the shared KEK,
    // which makes identified contributions undecryptable too — so the
    // brand-visibility (identified) tier MUST also be revoked, otherwise
    // `status` keeps reporting it ACTIVE from a stale brand_visibility optin row
    // even though its contributions can no longer be produced or read. Append a
    // brand_visibility optout when that tier is currently active. (Append-only;
    // if it is already inactive we leave the ledger untouched.)
    let bv_active = repo
        .latest_for_tier(project_id, ConsentTier::BrandVisibility)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to read brand-visibility state: {e}")))?
        .map(|r| r.is_active(TERMS_VERSION))
        .unwrap_or(false);
    if bv_active {
        repo.record_optout_tier(
            project_id,
            ConsentTier::BrandVisibility,
            TERMS_VERSION,
            actor.as_deref(),
            args.note.as_deref(),
        )
        .await
        .map_err(|e| {
            OpenGeoError::Config(format!(
                "opt-out recorded (event id {id}) but failed to revoke the brand-visibility \
                 tier: {e}. Re-run `ogeo benchmark optout` to complete the opt-out."
            ))
        })?;
    }

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

/// `ogeo benchmark optout --brand-visibility` (Story 44.1, AC4).
///
/// Single command, single "yes" confirmation (GDPR Art.7(3) — withdrawal is one
/// action). Appends a `tier = brand_visibility` optout event; future identified
/// contributions stop immediately. This is a SOFT withdrawal of the named-tier
/// claim — it deliberately does NOT crypto-shred the benchmark KEK, because the
/// anonymous aggregate tier may still be active and shares that key. To fully
/// erase aggregate contributions, run `ogeo benchmark optout` (no flag).
async fn run_optout_brand_visibility(args: OptoutArgs) -> Result<(), OpenGeoError> {
    let (storage, project_id) = open_storage(args.config.as_deref()).await?;

    if !args.yes {
        print!(
            "Withdraw from the brand-visibility (identified) tier? Your brand stops appearing \
             named in the leaderboard. Type `yes` to confirm: "
        );
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin()
            .lock()
            .read_line(&mut answer)
            .map_err(|e| OpenGeoError::Config(format!("failed to read confirmation: {e}")))?;
        if answer.trim() != "yes" {
            return Err(OpenGeoError::Config(
                "brand-visibility opt-out cancelled — type `yes` exactly to confirm. Your \
                 identified-tier consent is unchanged."
                    .into(),
            ));
        }
    }

    let actor = resolve_actor(args.actor);
    let id = BenchmarkConsentRepo::new(storage.pool())
        .record_optout_tier(
            project_id,
            ConsentTier::BrandVisibility,
            TERMS_VERSION,
            actor.as_deref(),
            args.note.as_deref(),
        )
        .await
        .map_err(|e| {
            OpenGeoError::Config(format!("failed to record brand-visibility opt-out: {e}"))
        })?;
    println!(
        "✓ Withdrawn from brand-visibility (identified) tier (event id {id}, terms version \
         {TERMS_VERSION}, at {}).",
        Utc::now().to_rfc3339()
    );
    println!(
        "  Future identified contributions have stopped. Your anonymous aggregate consent (if \
         active) is unaffected; run `ogeo benchmark optout` without --brand-visibility to also \
         crypto-shred the aggregate key."
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

    // Story 44.1: report the brand-visibility (identified) tier independently.
    let identified = BenchmarkConsentRepo::new(storage.pool())
        .latest_for_tier(project_id, ConsentTier::BrandVisibility)
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to read brand-visibility state: {e}")))?;
    match identified {
        None => println!("Brand-visibility (identified) tier: not opted in"),
        Some(row) => {
            let active = row.is_active(TERMS_VERSION);
            println!(
                "Brand-visibility (identified) tier: {} (last event: {} at {})",
                if active { "active" } else { "inactive" },
                row.event,
                row.created_at.to_rfc3339()
            );
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

/// Resolve the project's normalized domain from `brand.site_url` in the project
/// config. Used to gate the brand-visibility opt-in on a domain-verified claim
/// (Story 44.1). Errors if the config can't be read or no `site_url` is set —
/// you cannot make a named/ranked claim without a domain to verify.
fn project_domain(config: Option<&std::path::Path>) -> Result<String, OpenGeoError> {
    let path = config.unwrap_or(std::path::Path::new("anseo.yaml"));
    let cfg = anseo_core::Config::from_path(path).map_err(|e| {
        OpenGeoError::Config(format!(
            "failed to read project config at `{}`: {e}",
            path.display()
        ))
    })?;
    let site_url = cfg.brand.site_url.as_deref().ok_or_else(|| {
        OpenGeoError::Config(
            "brand-visibility requires a verified domain, but no `brand.site_url` is set in your \
             project config. Add your owned website URL and verify domain ownership first."
                .into(),
        )
    })?;
    Ok(EntityRepo::normalize_domain(site_url))
}
