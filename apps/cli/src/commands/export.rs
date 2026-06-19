//! `ogeo export bundle` — Story 27.7 self-host export.
//!
//! Emits a versioned, checksummed archive (projects, prompts, run history).
//! Provider keys are NEVER included (AC-2: write-only invariant).

use std::io::Write as _;
use std::path::PathBuf;

use anseo_core::OpenGeoError;
use anseo_storage::Storage;
use chrono::Utc;
use clap::{Args, Subcommand};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Subcommand)]
pub enum ExportSub {
    /// Export all brands, prompts, and run history to a portable archive.
    Bundle(BundleArgs),
}

#[derive(Debug, Args)]
pub struct BundleArgs {
    /// Output path. Defaults to `anseo-bundle-<ISO date>.json.gz` in cwd.
    #[arg(long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Override `DATABASE_URL`.
    #[arg(long, env = "DATABASE_URL", value_name = "URL")]
    pub database_url: Option<String>,
}

/// Top-level bundle structure written into the archive.
#[derive(Debug, Serialize)]
struct Bundle {
    /// Semver-style format version; parsers can reject unknown majors.
    bundle_version: &'static str,
    exported_at: chrono::DateTime<Utc>,
    projects: Vec<BundleProject>,
}

#[derive(Debug, Serialize)]
struct BundleProject {
    id: String,
    name: String,
    created_at: chrono::DateTime<Utc>,
    prompts: Vec<BundlePrompt>,
}

#[derive(Debug, Serialize)]
struct BundlePrompt {
    id: String,
    name: String,
    text: String,
    tags: Vec<String>,
    created_at: chrono::DateTime<Utc>,
    runs: Vec<BundleRun>,
}

#[derive(Debug, Serialize)]
struct BundleRun {
    id: String,
    provider: String,
    provider_model_version: String,
    status: String,
    started_at: chrono::DateTime<Utc>,
    finished_at: Option<chrono::DateTime<Utc>>,
    created_at: chrono::DateTime<Utc>,
}

pub async fn run_bundle(args: BundleArgs) -> Result<(), OpenGeoError> {
    let db_url = args
        .database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .ok_or_else(|| OpenGeoError::Config("DATABASE_URL not set".into()))?;

    let storage = Storage::connect(&db_url)
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("{e}")))?;

    // Collect all projects.
    let project_rows = storage
        .projects()
        .list_projects()
        .await
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("{e}")))?;

    let epoch = chrono::DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_default();

    let mut bundle_projects = Vec::with_capacity(project_rows.len());
    for proj in &project_rows {
        let prompt_rows = storage
            .prompts()
            .list_by_project(proj.id)
            .await
            .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("{e}")))?;

        let mut bundle_prompts = Vec::with_capacity(prompt_rows.len());
        for prompt in &prompt_rows {
            let run_rows = storage
                .prompt_runs()
                .list_by_prompt_since(prompt.id, epoch)
                .await
                .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("{e}")))?;

            let runs = run_rows
                .into_iter()
                .map(|r| BundleRun {
                    id: r.id.to_string(),
                    provider: r.provider,
                    provider_model_version: r.provider_model_version,
                    status: r.status,
                    started_at: r.started_at,
                    finished_at: r.finished_at,
                    created_at: r.created_at,
                })
                .collect();

            bundle_prompts.push(BundlePrompt {
                id: prompt.id.to_string(),
                name: prompt.name.clone(),
                text: prompt.text.clone(),
                tags: prompt.tags.clone(),
                created_at: prompt.created_at,
                runs,
            });
        }

        bundle_projects.push(BundleProject {
            id: proj.id.to_string(),
            name: proj.name.clone(),
            created_at: proj.created_at,
            prompts: bundle_prompts,
        });
    }

    let bundle = Bundle {
        bundle_version: "1.0",
        exported_at: Utc::now(),
        projects: bundle_projects,
    };

    let json = serde_json::to_vec(&bundle)
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("serialization: {e}")))?;

    // Checksum before compression.
    let checksum = hex::encode(Sha256::digest(&json));

    // Gzip-compress.
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&json)
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("gzip write: {e}")))?;
    let compressed = gz
        .finish()
        .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("gzip finish: {e}")))?;

    let out_path = args.output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "anseo-bundle-{}.json.gz",
            Utc::now().format("%Y%m%dT%H%M%SZ")
        ))
    });

    std::fs::write(&out_path, &compressed).map_err(|e| {
        OpenGeoError::Internal(anyhow::anyhow!("write {}: {e}", out_path.display()))
    })?;

    println!("Bundle written to {}", out_path.display());
    println!("SHA-256 (uncompressed): {checksum}");
    println!(
        "  {} project(s), {} prompt(s) exported. No provider keys included.",
        bundle.projects.len(),
        bundle
            .projects
            .iter()
            .map(|p| p.prompts.len())
            .sum::<usize>()
    );

    Ok(())
}
