//! `ogeo project …` — Story 36.6 CLI project verbs + the shared
//! project-selection precedence chain (ADR-004, CLI flavor).
//!
//! The CLI is a fat direct-DB tool (no HTTP client): every verb reads
//! `DATABASE_URL` and operates on the `projects` repo.
//!
//! Verbs:
//! - `list`   — print active (non-archived) projects, oldest first.
//! - `create <brand>` — derive a `project_id` from the brand name and insert.
//! - `use <id|name>`  — persist a selected project for the working dir, written
//!   to a small `.opengeo/selected-project` marker so subsequent verbs in that
//!   directory resolve to it without a flag.
//!
//! ## Precedence (ADR-004, CLI flavor)
//!
//! Resolved by [`resolve_project_id`], applied identically to every verb that
//! needs a project. Most-explicit wins:
//!
//! 1. explicit `--project <id|name>` flag
//! 2. `ogeo project use` selection (`.opengeo/selected-project` marker) OR the
//!    working-dir `opengeo.yaml` brand — the marker, when present, wins over the
//!    YAML; both are the "ambient working-dir" tier
//! 3. legacy sole active project (exactly one project in the DB)
//! 4. otherwise a clear error
//!
//! This mirrors the API/MCP resolver (`--project`/explicit arg > header >
//! working-dir > sole project > error) for cross-surface parity (36.10): the
//! CLI has no header tier, so the `use` marker / working-dir YAML occupies the
//! ambient slot the header fills on the API.

use std::io::{self, BufRead, Write};

use chrono::Utc;
use clap::Args;

use anseo_benchmark::{ProjectKek, TERMS_VERSION};
use anseo_core::{project_id_for_name, BrandConfig, Config, OpenGeoError, ProjectId};
use anseo_storage::models::ProjectRow;
use anseo_storage::repositories::benchmark_consent::BenchmarkConsentRepo;
use anseo_storage::Storage;

/// Marker directory written by `ogeo project use`, relative to the working dir.
const MARKER_DIR: &str = ".opengeo";
/// File inside [`MARKER_DIR`] holding the selected project id (ULID string).
const MARKER_FILE: &str = "selected-project";

// ---- args ---------------------------------------------------------------

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Output as JSON (machine-readable).
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Brand name. The `project_id` is derived from it (same identity the
    /// `opengeo.yaml` boot path uses), so re-creating the same brand is a
    /// conflict rather than a duplicate.
    pub brand: String,
    /// Optional brand-name variants/aliases to seed the project with.
    #[arg(long = "variant")]
    pub variants: Vec<String>,
    /// Optional owned-site URL.
    #[arg(long)]
    pub site_url: Option<String>,
}

#[derive(Debug, Args)]
pub struct UseArgs {
    /// Project id (ULID) or brand name to select for this working directory.
    pub project: String,
}

#[derive(Debug, Args)]
pub struct ShredArgs {
    /// Project id (ULID) or brand name whose benchmark key to destroy. Defaults
    /// to the working-dir selection / sole project when omitted.
    #[arg(long)]
    pub project: Option<String>,
    /// Skip the interactive confirmation. Because the shred is an IRREVERSIBLE
    /// cryptographic erasure, confirmation is required unless you pass this flag.
    #[arg(long)]
    pub yes: bool,
    /// Operator-facing actor identifier recorded in the audit log.
    #[arg(long)]
    pub actor: Option<String>,
    /// Free-form note recorded alongside the shred (opt-out) audit event.
    #[arg(long)]
    pub note: Option<String>,
}

// ---- list ---------------------------------------------------------------

pub async fn run_list(args: ListArgs) -> Result<(), OpenGeoError> {
    let storage = connect_storage().await?;
    let projects = storage.projects().list_projects().await.map_err(internal)?;
    if projects.is_empty() {
        if args.json {
            println!("[]");
        } else {
            println!("No projects yet. Create one with `ogeo project create <brand>`.");
        }
        return Ok(());
    }

    // Detect the currently-selected project for this working dir (marker tier).
    let cwd = std::env::current_dir()
        .map_err(|e| OpenGeoError::Config(format!("could not read current directory: {e}")))?;
    let selected = read_selection(&cwd).unwrap_or(None);

    if args.json {
        // Minimal JSON output: [{id, name, selected}]
        print!("[");
        for (i, p) in projects.iter().enumerate() {
            if i > 0 {
                print!(",");
            }
            let is_sel = selected == Some(p.id);
            print!(
                r#"{{"id":"{}","name":{},"selected":{}}}"#,
                p.id,
                serde_json::to_string(&p.name).expect("name is valid JSON"),
                is_sel
            );
        }
        println!("]");
    } else {
        for p in &projects {
            let marker = selected == Some(p.id);
            println!("{}{}  {}", if marker { "* " } else { "  " }, p.id, p.name);
        }
    }
    Ok(())
}

// ---- create -------------------------------------------------------------

pub async fn run_create(args: CreateArgs) -> Result<(), OpenGeoError> {
    let brand = BrandConfig {
        name: args.brand.clone(),
        variants: args.variants.clone(),
        site_url: args.site_url.clone(),
    };
    let derived = project_id_for_name(&brand.name);

    let storage = connect_storage().await?;
    // Guard against a duplicate so the derived-id collision surfaces as a clean
    // config error instead of a raw unique-violation from the DB layer.
    if storage
        .projects()
        .get_project(derived)
        .await
        .map_err(internal)?
        .is_some()
    {
        return Err(OpenGeoError::Config(format!(
            "a project named '{}' already exists ({derived})",
            brand.name
        )));
    }

    let id = storage
        .projects()
        .create_project(&brand)
        .await
        .map_err(internal)?;
    println!("Created project '{}' ({id})", brand.name);
    Ok(())
}

// ---- use ----------------------------------------------------------------

pub async fn run_use(args: UseArgs) -> Result<(), OpenGeoError> {
    let storage = connect_storage().await?;
    let projects = storage.projects().list_projects().await.map_err(internal)?;
    let row = match_project(&projects, &args.project).ok_or_else(|| {
        OpenGeoError::Config(format!(
            "no active project matches '{}' (try `ogeo project list`)",
            args.project
        ))
    })?;

    write_selection(
        &std::env::current_dir()
            .map_err(|e| OpenGeoError::Config(format!("could not read current directory: {e}")))?,
        row.id,
    )?;
    println!("Selected project '{}' ({})", row.name, row.id);
    Ok(())
}

// ---- shred ---------------------------------------------------------------

/// `ogeo project shred` (Story 40.4, AC-3) — crypto-shred the project's
/// benchmark KEK.
///
/// This is the operator-facing "right to erasure" lever for a whole project's
/// benchmark footprint, ingested or native alike. Destroying the single
/// per-project KEK (Story 39.1) makes every wrapped DEK — and therefore every
/// benchmark contribution this project ever sealed, INCLUDING those sealed from
/// `POST /v1/ingest/run` (Story 40.4) — permanently undecryptable. It is the
/// same `ProjectKek::destroy` mechanism `ogeo benchmark optout` uses; this verb
/// exposes it directly under the project namespace so an operator can shred
/// without first reasoning about consent tiers.
///
/// Order of operations mirrors the benchmark opt-out: record the audit (opt-out)
/// event FIRST so the intent is captured even if the key-destruction backend
/// errors, then destroy the key across every SecretStore leg.
pub async fn run_shred(args: ShredArgs) -> Result<(), OpenGeoError> {
    let storage = connect_storage().await?;
    let cwd = std::env::current_dir()
        .map_err(|e| OpenGeoError::Config(format!("could not read current directory: {e}")))?;
    let project_id = resolve_project_id(&storage, &cwd, args.project.as_deref()).await?;
    let project_str = project_id.to_string();

    eprintln!("⚠  IRREVERSIBLE DESTRUCTIVE ACTION — benchmark key crypto-shred");
    eprintln!(
        "    This destroys the per-project benchmark key for `{project_str}`. Every benchmark"
    );
    eprintln!(
        "    contribution this project ever sealed — native AND ingested (POST /v1/ingest/run)"
    );
    eprintln!("    — becomes permanently undecryptable. There is no recovery.");

    if !args.yes {
        print!("To confirm, type the project id `{project_str}` exactly: ");
        io::stdout().flush().ok();
        let mut answer = String::new();
        io::stdin()
            .lock()
            .read_line(&mut answer)
            .map_err(|e| OpenGeoError::Config(format!("failed to read confirmation: {e}")))?;
        if answer.trim() != project_str {
            return Err(OpenGeoError::Config(
                "shred cancelled — the project id was not entered exactly. Nothing was \
                 destroyed; the benchmark key and contributions are untouched. (Re-run with \
                 `--yes` to skip this prompt.)"
                    .into(),
            ));
        }
    }

    // Record the audit event FIRST (append-only opt-out on the anonymous tier),
    // so the erasure intent is durable even if key destruction surfaces an error.
    let actor = args
        .actor
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_else(|| "cli".to_string());
    let repo = BenchmarkConsentRepo::new(storage.pool());
    let id = repo
        .record_optout(
            project_id,
            TERMS_VERSION,
            Some(&actor),
            args.note.as_deref(),
        )
        .await
        .map_err(|e| OpenGeoError::Config(format!("failed to record shred audit event: {e}")))?;

    // CRYPTO-SHRED: destroy the per-project KEK across every SecretStore leg.
    // Idempotent — a project that never sealed a contribution simply has no KEK,
    // and that is still a successful, complete shred.
    let store = anseo_core::default_chain();
    ProjectKek::destroy(&store, &project_str).map_err(|e| {
        OpenGeoError::Config(format!(
            "shred audit recorded (event id {id}) but crypto-shred of the benchmark key FAILED: \
             {e}. The contributions are NOT yet cryptographically erased. Resolve the \
             secret-store backend error and re-run `ogeo project shred` to complete the erasure."
        ))
    })?;

    println!(
        "✓ Crypto-shredded benchmark key for project `{project_str}` (audit event id {id}, terms \
         version {TERMS_VERSION}, at {}).",
        Utc::now().to_rfc3339()
    );
    println!(
        "  Every benchmark contribution this project sealed — native and ingested — is now \
         permanently undecryptable."
    );
    Ok(())
}

// ---- shared precedence resolver -----------------------------------------

/// Resolve the active [`ProjectId`] for a CLI verb following ADR-004.
///
/// `flag` is the value of the global `--project <id|name>` flag (if any).
/// `working_dir` anchors the `.opengeo/selected-project` marker and the
/// `opengeo.yaml` lookup. `storage` is needed to translate a name → id, honor
/// the `use` selection, and fall back to a legacy sole project.
pub async fn resolve_project_id(
    storage: &Storage,
    working_dir: &std::path::Path,
    flag: Option<&str>,
) -> Result<ProjectId, OpenGeoError> {
    // 1. explicit `--project` flag wins.
    if let Some(sel) = flag {
        let projects = storage.projects().list_projects().await.map_err(internal)?;
        return match_project(&projects, sel).map(|r| r.id).ok_or_else(|| {
            OpenGeoError::Config(format!(
                "--project '{sel}' did not match any active project"
            ))
        });
    }

    // 2a. `ogeo project use` selection marker for this working dir.
    if let Some(id) = read_selection(working_dir)? {
        // Validate the marker still points at a live project; a stale marker
        // (project archived/purged) must fail loudly rather than silently fall
        // through to a different project.
        if storage
            .projects()
            .get_project(id)
            .await
            .map_err(internal)?
            .is_some()
        {
            return Ok(id);
        }
        return Err(OpenGeoError::Config(format!(
            "selected project {id} (.opengeo/selected-project) no longer exists; \
             re-run `ogeo project use`"
        )));
    }

    // 2b. working-dir `opengeo.yaml` brand.
    let yaml_path = working_dir.join("anseo.yaml");
    if yaml_path.exists() {
        let yaml = std::fs::read_to_string(&yaml_path).map_err(|e| {
            OpenGeoError::Config(format!("could not read {}: {e}", yaml_path.display()))
        })?;
        let config = Config::from_yaml_str(&yaml).map_err(|e| {
            OpenGeoError::Config(format!("could not parse {}: {e}", yaml_path.display()))
        })?;
        return Ok(config.project_id());
    }

    // 3. legacy sole active project.
    let projects = storage.projects().list_projects().await.map_err(internal)?;
    if projects.len() == 1 {
        return Ok(projects[0].id);
    }

    // 4. nothing resolved.
    Err(OpenGeoError::Config(
        "no project selected: pass --project <id|name>, run `ogeo project use`, \
         or work in a directory with an opengeo.yaml"
            .into(),
    ))
}

/// Resolve a project for a verb that has already loaded its `opengeo.yaml`
/// into a [`Config`], honoring the same ADR-004 chain as [`resolve_project_id`]
/// but using the in-hand config as the working-dir tier (no second YAML read).
///
/// Precedence: explicit `--project` flag > `ogeo project use` marker (current
/// dir) > the loaded `config`'s brand > legacy sole active project.
pub async fn resolve_with_config(
    storage: &Storage,
    config: &Config,
    flag: Option<&str>,
) -> Result<ProjectId, OpenGeoError> {
    if let Some(sel) = flag {
        let projects = storage.projects().list_projects().await.map_err(internal)?;
        return match_project(&projects, sel).map(|r| r.id).ok_or_else(|| {
            OpenGeoError::Config(format!(
                "--project '{sel}' did not match any active project"
            ))
        });
    }

    let cwd = std::env::current_dir()
        .map_err(|e| OpenGeoError::Config(format!("could not read current directory: {e}")))?;
    if let Some(id) = read_selection(&cwd)? {
        if storage
            .projects()
            .get_project(id)
            .await
            .map_err(internal)?
            .is_some()
        {
            return Ok(id);
        }
        return Err(OpenGeoError::Config(format!(
            "selected project {id} (.opengeo/selected-project) no longer exists; \
             re-run `ogeo project use`"
        )));
    }

    Ok(config.project_id())
}

// ---- helpers ------------------------------------------------------------

/// Match a `--project` / `use` selector against the project list by exact id
/// (ULID) first, then by exact brand name (case-insensitive, matching the
/// brand-name canonicalization used to derive ids).
fn match_project<'a>(projects: &'a [ProjectRow], selector: &str) -> Option<&'a ProjectRow> {
    if let Ok(id) = selector.parse::<ProjectId>() {
        if let Some(row) = projects.iter().find(|p| p.id == id) {
            return Some(row);
        }
    }
    projects
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(selector))
}

fn marker_path(working_dir: &std::path::Path) -> std::path::PathBuf {
    working_dir.join(MARKER_DIR).join(MARKER_FILE)
}

fn write_selection(working_dir: &std::path::Path, id: ProjectId) -> Result<(), OpenGeoError> {
    let path = marker_path(working_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            OpenGeoError::Config(format!("could not create {}: {e}", parent.display()))
        })?;
    }
    std::fs::write(&path, format!("{id}\n"))
        .map_err(|e| OpenGeoError::Config(format!("could not write {}: {e}", path.display())))?;
    Ok(())
}

/// Read the persisted selection for `working_dir`, if any. A missing marker is
/// `Ok(None)`; a present-but-garbage marker is a clean config error.
fn read_selection(working_dir: &std::path::Path) -> Result<Option<ProjectId>, OpenGeoError> {
    let path = marker_path(working_dir);
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let trimmed = s.trim();
            let id = trimmed.parse::<ProjectId>().map_err(|_| {
                OpenGeoError::Config(format!(
                    "{} holds an invalid project id ('{trimmed}')",
                    path.display()
                ))
            })?;
            Ok(Some(id))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(OpenGeoError::Config(format!(
            "could not read {}: {e}",
            path.display()
        ))),
    }
}

async fn connect_storage() -> Result<Storage, OpenGeoError> {
    let url = std::env::var("DATABASE_URL")
        .map_err(|_| OpenGeoError::Config("DATABASE_URL is required for `ogeo project`".into()))?;
    let storage = Storage::connect(&url)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    storage
        .migrate()
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    Ok(storage)
}

fn internal(e: anseo_storage::Error) -> OpenGeoError {
    OpenGeoError::Internal(anyhow::anyhow!(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn row(name: &str) -> ProjectRow {
        let id = project_id_for_name(name);
        ProjectRow {
            id,
            name: name.to_string(),
            organization_id: None,
            tenant_id: None,
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn match_by_name_is_case_insensitive() {
        let projects = vec![row("Sunski"), row("Acme")];
        let found = match_project(&projects, "sunski").unwrap();
        assert_eq!(found.name, "Sunski");
    }

    #[test]
    fn match_by_id_wins() {
        let projects = vec![row("Sunski"), row("Acme")];
        let id = project_id_for_name("Acme");
        let found = match_project(&projects, &id.to_string()).unwrap();
        assert_eq!(found.name, "Acme");
    }

    #[test]
    fn match_unknown_is_none() {
        let projects = vec![row("Sunski")];
        assert!(match_project(&projects, "nope").is_none());
    }

    #[test]
    fn selection_roundtrips_through_marker() {
        let dir = TempDir::new().unwrap();
        let id = project_id_for_name("Sunski");
        write_selection(dir.path(), id).unwrap();
        let read = read_selection(dir.path()).unwrap();
        assert_eq!(read, Some(id));
    }

    #[test]
    fn missing_marker_reads_none() {
        let dir = TempDir::new().unwrap();
        assert_eq!(read_selection(dir.path()).unwrap(), None);
    }

    #[test]
    fn garbage_marker_is_config_error() {
        let dir = TempDir::new().unwrap();
        let path = marker_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not-a-ulid\n").unwrap();
        assert!(read_selection(dir.path()).is_err());
    }
}
