//! `ogeo prompt run` — FR-2, FR-6, FR-13.
//!
//! Loads `opengeo.yaml`, resolves provider secrets via the chained secret
//! store, builds a [`ProviderRegistry`], and dispatches to
//! [`anseo_providers::Orchestrator`]. Returns records via the
//! `PromptRunSink` so a callback can persist them (Story 3.1 will plug in
//! a Postgres-backed sink; for now we write a JSON line per record to stdout
//! plus a summary line to stderr).
//!
//! Exit-code semantics per PRD §11.4 / FR-2:
//! - 0 if at least one provider succeeded
//! - 2 if every cell failed with a provider error
//! - 70 if the orchestrator itself errored before issuing any call
//!   (mapped via `OpenGeoError::Internal`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anseo_core::{Config, OpenGeoError, ProviderName};
use anseo_providers::{
    persistence::persist_records, registry::build_real_registry as build_real_registry_inner,
    MockProvider, Orchestrator, OrchestratorFilter, PromptRunRecord, PromptRunStatus,
    ProviderRegistry, RunSummary,
};
use anseo_storage::Storage;
use clap::Args;

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Run only the named prompt. Repeatable.
    #[arg(long)]
    pub prompt: Vec<String>,

    /// Run only against the named provider. Repeatable.
    #[arg(long)]
    pub provider: Vec<String>,

    /// Path to opengeo.yaml. Defaults to `./opengeo.yaml`.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Skip real provider HTTP and use a deterministic MockProvider that
    /// returns canned responses. Used by smoke tests and screenshot fixtures.
    #[arg(long)]
    pub use_mock_provider: bool,
}

pub async fn run(args: RunArgs) -> Result<(), OpenGeoError> {
    let config_path = args.config.unwrap_or_else(|| PathBuf::from("anseo.yaml"));
    let config_path = Config::auto_migrate_config_filename(&config_path, "opengeo.yaml");
    let config = Config::from_path(&config_path)?;
    let filter = build_filter(&args.prompt, &args.provider)?;

    let registry = if args.use_mock_provider {
        build_mock_registry(&config)
    } else {
        build_real_registry(&config)?
    };

    let orchestrator = Orchestrator::new(config.clone(), registry);
    let records = orchestrator.run_all(filter).await;
    let summary = RunSummary::from_records(&records);

    emit_records(&records);
    emit_summary(&summary);

    // Persist when DATABASE_URL is available. Without it, the run is fire-and-
    // forget — useful for `--use-mock-provider` smoke tests in CI and dry-run
    // shell pipelines.
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        let storage = Storage::connect(&database_url).await.map_err(|e| {
            OpenGeoError::Internal(anyhow::anyhow!("connecting to {database_url}: {e}"))
        })?;
        storage
            .migrate()
            .await
            .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("migrate: {e}")))?;
        persist_records(&storage, &config, &records)
            .await
            .map_err(|e| OpenGeoError::Internal(anyhow::anyhow!("persist: {e}")))?;
        eprintln!("Persisted {} runs to the database.", records.len());
    }

    // Exit code mapping (FR-2):
    if summary.total == 0 {
        return Err(OpenGeoError::Config(
            "no (prompt, provider) cells to run — check filters and opengeo.yaml".into(),
        ));
    }
    if summary.succeeded == 0 {
        // Every cell failed. Surface as ProviderError so exit code is 2.
        let first = records
            .iter()
            .find(|r| r.status == PromptRunStatus::Failed)
            .expect("non-zero total and zero succeeded implies at least one failure");
        return Err(OpenGeoError::Provider {
            kind: first
                .error_kind
                .unwrap_or(anseo_core::ProviderErrorKind::NetworkError),
            message: first
                .error_message
                .clone()
                .unwrap_or_else(|| "all provider calls failed".into()),
        });
    }
    Ok(())
}

fn build_filter(
    prompts: &[String],
    providers: &[String],
) -> Result<OrchestratorFilter, OpenGeoError> {
    let prompt_names = if prompts.is_empty() {
        None
    } else {
        Some(prompts.to_vec())
    };
    let providers_parsed = if providers.is_empty() {
        None
    } else {
        let mut out = Vec::new();
        for p in providers {
            match ProviderName::parse(p) {
                Some(provider) => out.push(provider),
                None => {
                    return Err(OpenGeoError::Config(format!(
                        "unsupported --provider `{p}`; expected one of {}",
                        ProviderName::all_wire_names().join(", ")
                    )))
                }
            }
        }
        Some(out)
    };
    Ok(OrchestratorFilter {
        prompt_names,
        providers: providers_parsed,
    })
}

fn build_real_registry(config: &Config) -> Result<ProviderRegistry, OpenGeoError> {
    // Shared with the API path; see `crates/providers/src/registry.rs`.
    // Missing-secret providers are omitted from the registry; the
    // orchestrator synthesises a `failed` record for them on dispatch.
    build_real_registry_inner(config).map_err(|e| OpenGeoError::Auth(e.to_string()))
}

fn build_mock_registry(config: &Config) -> ProviderRegistry {
    let mut registry: ProviderRegistry = HashMap::new();
    let prompt_count = config.prompts.len().max(1);
    for provider_cfg in &config.providers {
        // Mirror the orchestrator's model resolution: OpenRouter may fan out
        // across a `models` list; everyone else resolves to a single model.
        let models: Vec<String> = match &provider_cfg.models {
            Some(ms) if !ms.is_empty() => ms.clone(),
            _ => vec![provider_cfg
                .model
                .clone()
                .unwrap_or_else(|| provider_cfg.name.default_model().to_string())],
        };
        let canned = format!(
            "[MOCK {}] Acme is the leading example brand, ahead of Beta Corp and Gamma Labs.",
            provider_cfg.name
        );
        let mut provider = MockProvider::new(provider_cfg.name.clone());
        for m in &models {
            provider = provider.accept_model(m);
        }
        // One dispatch per (prompt × model); queue that many canned responses.
        for _ in 0..(prompt_count * models.len()) {
            provider = provider.queue_response(canned.clone());
        }
        registry.insert(provider_cfg.name.clone(), Arc::new(provider));
    }
    registry
}

fn emit_records(records: &[PromptRunRecord]) {
    for r in records {
        let line = serde_json::json!({
            "id": r.id.to_string(),
            "prompt_name": r.prompt_name,
            "provider": r.provider.as_wire_str(),
            "model": r.provider_model_version,
            "status": r.status.as_wire_str(),
            "started_at": r.started_at.to_rfc3339(),
            "finished_at": r.finished_at.map(|t| t.to_rfc3339()),
            "error_kind": r.error_kind.map(|k| k.as_wire_str()),
        });
        println!("{line}");
    }
}

fn emit_summary(summary: &RunSummary) {
    eprintln!(
        "Run complete: {}/{} succeeded, {} failed.",
        summary.succeeded, summary.total, summary.failed
    );
}
