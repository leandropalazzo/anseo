//! `ogeo recommend …` — Story 19.7 GEO recommendation CLI verbs.
//!
//! Storage-direct admin surface (mirrors `ogeo webhook`): every verb reads
//! `DATABASE_URL` and operates on the `recommendations` repo. Verbs:
//! - `generate` — assemble live project facts, run the in-process engine, and
//!   persist the result (dedup-aware), mirroring `POST /v1/recommendations/generate`.
//! - `list` — print active recommendations, newest first.
//! - `show --id <uuid>` — full row for one recommendation (scoped to project).
//! - `ack --id <uuid>` — Surfaced -> Acknowledged.
//! - `dismiss --id <uuid>` — -> Dismissed.
//! - `mark-acted --id <uuid> [--evidence-url <url>] [--note <text>]` —
//!   Acknowledged -> Acted; prints a `lifecycle.evidence_missing` warning when
//!   no evidence is supplied (decision L4 / UX-DR110).

use chrono::{Duration, Utc};
use clap::Args;
use uuid::Uuid;

use anseo_core::{Config, OpenGeoError};
use anseo_recommendations::assembly::{self, ProjectFacts, PromptFacts, PromptRunFacts};
use anseo_recommendations::lifecycle::{self, State as LifecycleState};
use anseo_recommendations::{Engine, Recommendation};
use anseo_storage::repositories::recommendations::{NewRecommendation, RecommendationRow};
use anseo_storage::Storage;
use serde::Serialize;

/// Evaluation window for a generation run (architecture §2: 14 days).
const WINDOW_DAYS: i64 = 14;

#[derive(Debug, Args)]
pub struct GenerateArgs {
    #[arg(long, default_value = "anseo.yaml")]
    pub config: std::path::PathBuf,
    /// Select the project by id (ULID) or brand name, overriding the working-dir
    /// `anseo.yaml` (ADR-004). Populated from the global `--project` flag.
    #[arg(skip)]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    #[arg(long, default_value = "anseo.yaml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    #[arg(long)]
    pub id: String,
    #[arg(long, default_value = "anseo.yaml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct AckArgs {
    #[arg(long)]
    pub id: String,
    #[arg(long, default_value = "anseo.yaml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct DismissArgs {
    #[arg(long)]
    pub id: String,
    #[arg(long, default_value = "anseo.yaml")]
    pub config: std::path::PathBuf,
}

#[derive(Debug, Args)]
pub struct MarkActedArgs {
    #[arg(long)]
    pub id: String,
    #[arg(long)]
    pub evidence_url: Option<String>,
    #[arg(long)]
    pub note: Option<String>,
    #[arg(long, default_value = "anseo.yaml")]
    pub config: std::path::PathBuf,
}

// ---- generate -----------------------------------------------------------

pub async fn run_generate(args: GenerateArgs) -> Result<(), OpenGeoError> {
    let yaml = std::fs::read_to_string(&args.config).map_err(|e| {
        OpenGeoError::Config(format!("could not read {}: {e}", args.config.display()))
    })?;
    let config = Config::from_yaml_str(&yaml).map_err(|e| {
        OpenGeoError::Config(format!("could not parse {}: {e}", args.config.display()))
    })?;
    let storage = connect_storage().await?;
    let project_id =
        super::project::resolve_with_config(&storage, &config, args.project.as_deref()).await?;

    let now = Utc::now();
    let window_start = now - Duration::days(WINDOW_DAYS);
    let project_ulid = project_id.into_ulid();

    let db_prompts = storage
        .prompts()
        .list_by_project(project_id)
        .await
        .map_err(internal)?;

    let mut prompt_facts = Vec::with_capacity(db_prompts.len());
    for prompt in &db_prompts {
        let runs = storage
            .prompt_runs()
            .list_by_prompt_since(prompt.id, window_start)
            .await
            .map_err(internal)?;
        let mut run_facts = Vec::with_capacity(runs.len());
        for run in &runs {
            let citations = storage
                .citations()
                .list_by_run(run.id)
                .await
                .map_err(internal)?;
            run_facts.push(PromptRunFacts {
                run_id: run.id.into_ulid(),
                citation_domains: citations.iter().map(|c| c.domain.clone()).collect(),
                citation_ids: citations.iter().map(|c| c.id.into_ulid()).collect(),
            });
        }
        prompt_facts.push(PromptFacts {
            prompt_id: prompt.id.into_ulid(),
            prompt: prompt.text.clone(),
            runs: run_facts,
        });
    }

    let facts = ProjectFacts {
        project_id: project_ulid,
        brand: config.brand.name.clone(),
        brand_etld1: String::new(),
        docs_etld1: None,
        competitors: config.competitors.iter().map(|c| c.name.clone()).collect(),
        enabled_providers: config
            .providers
            .iter()
            .map(|p| p.name.as_wire_str().into_owned())
            .collect(),
        benchmark_opted_in: false,
        prompts: prompt_facts,
        window: anseo_recommendations::wire::TimeWindow {
            start: window_start,
            end: now,
        },
        generated_at: now,
    };

    let input = assembly::assemble(facts);
    let recs = Engine::default().generate(&input);
    let generated_count = recs.len();

    let project_uuid = Uuid::from_bytes(project_ulid.to_bytes());
    let mut inserted_count = 0usize;
    for rec in &recs {
        let inserted = storage
            .recommendations()
            .insert(rec_to_new_row(rec, project_uuid))
            .await
            .map_err(internal)?;
        if inserted.is_some() {
            inserted_count += 1;
        }
    }

    println!(
        "Generated {generated_count} recommendation(s); {inserted_count} newly persisted (dedup dropped {}).",
        generated_count - inserted_count
    );
    Ok(())
}

// ---- list ---------------------------------------------------------------

pub async fn run_list(args: ListArgs) -> Result<(), OpenGeoError> {
    let project_uuid = project_uuid_from_config(&args.config)?;
    let storage = connect_storage().await?;
    let rows = storage
        .recommendations()
        .find_active_by_project(project_uuid)
        .await
        .map_err(internal)?;

    if rows.is_empty() {
        println!("(no active recommendations for this project)");
        return Ok(());
    }

    println!(
        "{:<28} {:<22} {:<14} {:<12} SUMMARY",
        "ID", "KIND", "STATE", "SEVERITY"
    );
    for row in &rows {
        let id = uuid_to_ulid_string(row.id);
        let ndp = if row.tags.iter().any(|t| t == "non_deterministic_pipeline") {
            " [NDP]"
        } else {
            ""
        };
        println!(
            "{:<28} {:<22} {:<14} {:<12} {}{}",
            id, row.kind, row.state, row.severity, row.summary, ndp
        );
    }
    Ok(())
}

// ---- show ---------------------------------------------------------------

pub async fn run_show(args: ShowArgs) -> Result<(), OpenGeoError> {
    let project_uuid = project_uuid_from_config(&args.config)?;
    let storage = connect_storage().await?;
    let rec_id = parse_uuid(&args.id)?;

    let row = storage
        .recommendations()
        .find_by_id(rec_id)
        .await
        .map_err(internal)?
        .filter(|r| r.project_id == project_uuid)
        .ok_or_else(|| OpenGeoError::Config(format!("recommendation `{}` not found", args.id)))?;

    let json = serde_json::to_string_pretty(&row_to_json(&row))
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    println!("{json}");
    Ok(())
}

// ---- transitions: ack / dismiss / mark-acted ----------------------------

pub async fn run_ack(args: AckArgs) -> Result<(), OpenGeoError> {
    transition_cli(
        &args.config,
        &args.id,
        LifecycleState::Acknowledged,
        None,
        None,
    )
    .await
}

pub async fn run_dismiss(args: DismissArgs) -> Result<(), OpenGeoError> {
    transition_cli(
        &args.config,
        &args.id,
        LifecycleState::Dismissed,
        None,
        None,
    )
    .await
}

pub async fn run_mark_acted(args: MarkActedArgs) -> Result<(), OpenGeoError> {
    transition_cli(
        &args.config,
        &args.id,
        LifecycleState::Acted,
        args.evidence_url.as_deref(),
        args.note.as_deref(),
    )
    .await
}

async fn transition_cli(
    config: &std::path::Path,
    id: &str,
    to: LifecycleState,
    evidence_url: Option<&str>,
    note: Option<&str>,
) -> Result<(), OpenGeoError> {
    let project_uuid = project_uuid_from_config(config)?;
    let storage = connect_storage().await?;
    let rec_id = parse_uuid(id)?;

    let row = storage
        .recommendations()
        .find_by_id(rec_id)
        .await
        .map_err(internal)?
        .filter(|r| r.project_id == project_uuid)
        .ok_or_else(|| OpenGeoError::Config(format!("recommendation `{id}` not found")))?;

    let from = parse_state(&row.state).ok_or_else(|| {
        OpenGeoError::Internal(anyhow::anyhow!("stored state `{}` is not valid", row.state))
    })?;

    let mut warnings = Vec::new();
    let new_state = if to == LifecycleState::Acted {
        let result = lifecycle::mark_acted(from, note, evidence_url)
            .map_err(|e| OpenGeoError::Config(format!("illegal transition: {e}")))?;
        for w in result.warnings {
            warnings.push(w.kind.to_string());
        }
        result.state
    } else {
        lifecycle::transition(from, to)
            .map_err(|e| OpenGeoError::Config(format!("illegal transition: {e}")))?
    };

    storage
        .recommendations()
        .update_state(rec_id, project_uuid, new_state.as_str())
        .await
        .map_err(internal)?
        .ok_or_else(|| OpenGeoError::Config(format!("recommendation `{id}` not found")))?;

    println!(
        "Recommendation `{id}`: {} -> {}.",
        from.as_str(),
        new_state.as_str()
    );
    for w in &warnings {
        println!("    warning: {w}");
    }
    Ok(())
}

// ---- helpers ------------------------------------------------------------

fn internal(e: anseo_storage::Error) -> OpenGeoError {
    OpenGeoError::Internal(anyhow::anyhow!(e))
}

fn parse_uuid(s: &str) -> Result<Uuid, OpenGeoError> {
    Uuid::parse_str(s).map_err(|_| OpenGeoError::Config(format!("`{s}` is not a valid UUID")))
}

fn project_uuid_from_config(path: &std::path::Path) -> Result<Uuid, OpenGeoError> {
    let yaml = std::fs::read_to_string(path)
        .map_err(|e| OpenGeoError::Config(format!("could not read {}: {e}", path.display())))?;
    let cfg = Config::from_yaml_str(&yaml)
        .map_err(|e| OpenGeoError::Config(format!("could not parse {}: {e}", path.display())))?;
    Ok(Uuid::from_bytes(cfg.project_id().into_ulid().to_bytes()))
}

async fn connect_storage() -> Result<Storage, OpenGeoError> {
    let url = std::env::var("DATABASE_URL").map_err(|_| {
        OpenGeoError::Config("DATABASE_URL is required for `ogeo recommend`".into())
    })?;
    let storage = Storage::connect(&url)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    storage
        .migrate()
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!(e)))?;
    Ok(storage)
}

fn parse_state(s: &str) -> Option<LifecycleState> {
    match s {
        "generated" => Some(LifecycleState::Generated),
        "surfaced" => Some(LifecycleState::Surfaced),
        "acknowledged" => Some(LifecycleState::Acknowledged),
        "acted" => Some(LifecycleState::Acted),
        "measured" => Some(LifecycleState::Measured),
        "dismissed" => Some(LifecycleState::Dismissed),
        "stale" => Some(LifecycleState::Stale),
        _ => None,
    }
}

fn enum_str<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn rec_to_new_row(rec: &Recommendation, project_id: Uuid) -> NewRecommendation {
    NewRecommendation {
        id: Uuid::from_bytes(rec.id.to_bytes()),
        project_id,
        kind: rec.kind.as_str().to_string(),
        severity: enum_str(&rec.severity),
        confidence_band: enum_str(&rec.confidence_band),
        state: "generated".to_string(),
        summary: rec.summary.clone(),
        payload: rec.payload.clone(),
        traceability: serde_json::to_value(&rec.traceability).unwrap_or(serde_json::Value::Null),
        reproducibility_class: enum_str(&rec.reproducibility.class),
        reproducibility_note: rec.reproducibility.note.clone(),
        tags: rec.tags.clone(),
        input_fingerprint: rec.traceability.input_fingerprint.clone(),
        engine_version: rec.engine_version.clone(),
        plugin_source: None,
    }
}

fn row_to_json(row: &RecommendationRow) -> serde_json::Value {
    serde_json::json!({
        "id": uuid_to_ulid_string(row.id),
        "project_id": uuid_to_ulid_string(row.project_id),
        "kind": row.kind,
        "severity": row.severity,
        "confidence_band": row.confidence_band,
        "state": row.state,
        "summary": row.summary,
        "payload": row.payload,
        "traceability": row.traceability,
        "reproducibility": {
            "class": row.reproducibility_class,
            "note": row.reproducibility_note,
        },
        "tags": row.tags,
        "generated_at": row.generated_at,
        "engine_version": row.engine_version,
    })
}

fn uuid_to_ulid_string(u: Uuid) -> String {
    ulid::Ulid::from_bytes(u.into_bytes()).to_string()
}
