//! Prompt-run orchestration (FR-2, FR-6, NFR-4 failure isolation).
//!
//! The orchestrator takes a parsed [`Config`] and a registry of [`Provider`]
//! instances and runs the configured Prompt × Provider matrix, with bounded
//! concurrency. Every cell of the matrix produces a [`PromptRunRecord`]:
//! success and failure outcomes both land in the result vector so the caller
//! can persist (Story 3.1) and report on the run as a whole.
//!
//! # Failure isolation (NFR-4)
//!
//! Each (Prompt, Provider) future runs independently. A panic, error, or
//! cancellation in one cell never aborts a sibling. Errors are caught and
//! mapped onto [`anseo_core::ProviderErrorKind`] so callers can summarise.
//!
//! # Concurrency
//!
//! `Orchestrator::run_all` uses a [`tokio::sync::Semaphore`] sized at the
//! `concurrency` value from the config (default 4 per Story 2.1). All
//! provider calls share that semaphore — so the user can dial down
//! parallelism by editing `concurrency:` in opengeo.yaml.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::Semaphore;

use anseo_core::{
    Config, ProjectId, PromptId, PromptRunId, ProviderErrorKind, ProviderName, RequestId,
};

use crate::{Provider, ProviderError, ProviderRequest, ProviderResponse};

/// One row of the matrix output. Shape mirrors the FR-2 `prompt_runs`
/// column manifest so Story 3.1 can persist with one mapping per field.
#[derive(Debug, Clone)]
pub struct PromptRunRecord {
    pub id: PromptRunId,
    pub project_id: ProjectId,
    pub prompt_id: PromptId,
    pub prompt_name: String,
    pub provider: ProviderName,
    pub provider_model_version: String,
    pub provider_region: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub raw_response: serde_json::Value,
    pub request_parameters: serde_json::Value,
    pub message_text: Option<String>,
    pub status: PromptRunStatus,
    pub error_kind: Option<ProviderErrorKind>,
    pub error_message: Option<String>,
    pub request_id: RequestId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptRunStatus {
    Ok,
    Failed,
}

impl PromptRunStatus {
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
        }
    }
}

/// Aggregate counts used by the CLI to decide its exit code per FR-2.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
}

impl RunSummary {
    pub fn from_records(records: &[PromptRunRecord]) -> Self {
        let mut s = RunSummary {
            total: records.len(),
            ..Default::default()
        };
        for r in records {
            match r.status {
                PromptRunStatus::Ok => s.succeeded += 1,
                PromptRunStatus::Failed => s.failed += 1,
            }
        }
        s
    }
}

/// Filter applied before the matrix expands.
#[derive(Debug, Clone, Default)]
pub struct OrchestratorFilter {
    pub prompt_names: Option<Vec<String>>,
    pub providers: Option<Vec<ProviderName>>,
}

impl OrchestratorFilter {
    pub fn matches_prompt(&self, name: &str) -> bool {
        match &self.prompt_names {
            None => true,
            Some(allow) => allow.iter().any(|s| s == name),
        }
    }
    pub fn matches_provider(&self, p: &ProviderName) -> bool {
        match &self.providers {
            None => true,
            Some(allow) => allow.contains(p),
        }
    }
}

/// A registered provider instance keyed by [`ProviderName`].
pub type ProviderRegistry = HashMap<ProviderName, Arc<dyn Provider>>;

pub struct Orchestrator {
    config: Config,
    providers: ProviderRegistry,
}

impl Orchestrator {
    pub fn new(config: Config, providers: ProviderRegistry) -> Self {
        Self { config, providers }
    }

    /// Run every (declared prompt) × (declared provider) cell. The result
    /// contains one record per cell, regardless of success/failure.
    pub async fn run_all(&self, filter: OrchestratorFilter) -> Vec<PromptRunRecord> {
        let project_id = self.config.project_id();
        let concurrency_cap = self.config.concurrency.max(1) as usize;
        let semaphore = Arc::new(Semaphore::new(concurrency_cap));

        let mut handles = Vec::new();

        for prompt in self.config.prompts.iter() {
            if !filter.matches_prompt(&prompt.name) {
                continue;
            }
            let prompt_id = self
                .config
                .prompt_id(&prompt.name)
                .expect("declared prompts always resolve to an id");

            for provider_cfg in self.config.providers.iter() {
                if !filter.matches_provider(&provider_cfg.name) {
                    continue;
                }

                // Resolve the model list. OpenRouter may declare a `models`
                // list to fan one key out across multiple `<vendor>/<model>`
                // upstreams — each becomes its own run. Everyone else (and
                // OpenRouter with a single `model`) resolves to one model.
                let models: Vec<String> = match &provider_cfg.models {
                    Some(ms) if !ms.is_empty() => ms.clone(),
                    _ => vec![provider_cfg
                        .model
                        .clone()
                        .unwrap_or_else(|| provider_cfg.name.default_model().to_string())],
                };

                let maybe_provider = self.providers.get(&provider_cfg.name).cloned();

                for model in models {
                    let Some(provider) = maybe_provider.clone() else {
                        // Provider declared in config but no instance registered
                        // (e.g. missing API key). Synthesise a failed record per
                        // (prompt, provider, model) so the output covers the full
                        // matrix.
                        handles.push(tokio::spawn({
                            let prompt_name = prompt.name.clone();
                            let provider_name = provider_cfg.name.clone();
                            async move {
                                unregistered_record(
                                    project_id,
                                    prompt_id,
                                    prompt_name,
                                    provider_name,
                                    model,
                                )
                            }
                        }));
                        continue;
                    };

                    let prompt_text = prompt.text.clone();
                    let prompt_name = prompt.name.clone();
                    let provider_name = provider_cfg.name.clone();
                    let timeout = Duration::from_secs(provider_cfg.timeout_seconds.max(1));
                    let semaphore = semaphore.clone();

                    handles.push(tokio::spawn(async move {
                        let _permit = semaphore
                            .acquire_owned()
                            .await
                            .expect("semaphore is never closed");
                        execute_single(
                            provider,
                            project_id,
                            prompt_id,
                            prompt_name,
                            provider_name,
                            model,
                            prompt_text,
                            timeout,
                        )
                        .await
                    }));
                }
            }
        }

        let mut out = Vec::with_capacity(handles.len());
        for h in handles {
            match h.await {
                Ok(record) => out.push(record),
                Err(join_err) => {
                    // A panic inside a worker future. We surface it as an
                    // internal failure record so the matrix output is still
                    // complete (NFR-4 failure isolation).
                    out.push(panic_record(join_err));
                }
            }
        }

        // Stable ordering: prompt declaration order, then provider order.
        out.sort_by_key(|r| (r.prompt_name.clone(), r.provider.as_wire_str()));
        out
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_single(
    provider: Arc<dyn Provider>,
    project_id: ProjectId,
    prompt_id: PromptId,
    prompt_name: String,
    provider_name: ProviderName,
    model: String,
    prompt_text: String,
    timeout: Duration,
) -> PromptRunRecord {
    let request_id = RequestId::new();
    let started_at = Utc::now();
    let run_id = PromptRunId::new();

    let validated_model = match provider.validate_model(&model) {
        Ok(m) => m,
        Err(err) => {
            return failure_record(
                run_id,
                project_id,
                prompt_id,
                prompt_name,
                provider_name,
                model.clone(),
                started_at,
                err,
                request_id,
            );
        }
    };

    let request = ProviderRequest::new(prompt_text, &validated_model).with_timeout(timeout);
    let request_parameters = request.request_parameters.clone();

    let outcome = provider.run(request).await;
    let finished_at = Some(Utc::now());

    match outcome {
        Ok(response) => success_record(
            run_id,
            project_id,
            prompt_id,
            prompt_name,
            provider_name,
            response,
            started_at,
            finished_at,
            request_parameters,
            request_id,
        ),
        Err(err) => PromptRunRecord {
            id: run_id,
            project_id,
            prompt_id,
            prompt_name,
            provider: provider_name,
            provider_model_version: validated_model,
            provider_region: None,
            started_at,
            finished_at,
            raw_response: serde_json::json!({}),
            request_parameters,
            message_text: None,
            status: PromptRunStatus::Failed,
            error_kind: Some(err.kind),
            error_message: Some(err.message),
            request_id,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn success_record(
    id: PromptRunId,
    project_id: ProjectId,
    prompt_id: PromptId,
    prompt_name: String,
    provider_name: ProviderName,
    response: ProviderResponse,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    request_parameters: serde_json::Value,
    request_id: RequestId,
) -> PromptRunRecord {
    PromptRunRecord {
        id,
        project_id,
        prompt_id,
        prompt_name,
        provider: provider_name,
        provider_model_version: response.model,
        provider_region: response.region,
        started_at,
        finished_at,
        raw_response: response.raw_response,
        request_parameters,
        message_text: Some(response.message_text),
        status: PromptRunStatus::Ok,
        error_kind: None,
        error_message: None,
        request_id,
    }
}

#[allow(clippy::too_many_arguments)]
fn failure_record(
    id: PromptRunId,
    project_id: ProjectId,
    prompt_id: PromptId,
    prompt_name: String,
    provider_name: ProviderName,
    model: String,
    started_at: DateTime<Utc>,
    err: ProviderError,
    request_id: RequestId,
) -> PromptRunRecord {
    PromptRunRecord {
        id,
        project_id,
        prompt_id,
        prompt_name,
        provider: provider_name,
        provider_model_version: model,
        provider_region: None,
        started_at,
        finished_at: Some(Utc::now()),
        raw_response: serde_json::json!({}),
        request_parameters: serde_json::json!({}),
        message_text: None,
        status: PromptRunStatus::Failed,
        error_kind: Some(err.kind),
        error_message: Some(err.message),
        request_id,
    }
}

fn unregistered_record(
    project_id: ProjectId,
    prompt_id: PromptId,
    prompt_name: String,
    provider_name: ProviderName,
    model: String,
) -> PromptRunRecord {
    let now = Utc::now();
    PromptRunRecord {
        id: PromptRunId::new(),
        project_id,
        prompt_id,
        prompt_name,
        provider: provider_name.clone(),
        provider_model_version: model,
        provider_region: None,
        started_at: now,
        finished_at: Some(now),
        raw_response: serde_json::json!({}),
        request_parameters: serde_json::json!({}),
        message_text: None,
        status: PromptRunStatus::Failed,
        error_kind: Some(ProviderErrorKind::ProviderUnauthorized),
        error_message: Some(format!(
            "no client registered for `{provider_name}`; run `ogeo login {provider_name}`"
        )),
        request_id: RequestId::new(),
    }
}

fn panic_record(err: tokio::task::JoinError) -> PromptRunRecord {
    let now = Utc::now();
    PromptRunRecord {
        id: PromptRunId::new(),
        project_id: ProjectId::new(),
        prompt_id: PromptId::new(),
        prompt_name: "<panic>".into(),
        provider: ProviderName::Openai,
        provider_model_version: String::new(),
        provider_region: None,
        started_at: now,
        finished_at: Some(now),
        raw_response: serde_json::json!({}),
        request_parameters: serde_json::json!({}),
        message_text: None,
        status: PromptRunStatus::Failed,
        error_kind: Some(ProviderErrorKind::NetworkError),
        error_message: Some(format!("worker task panicked: {err}")),
        request_id: RequestId::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockProvider;
    use anseo_core::Config;

    const YAML: &str = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: p1
    text: "first prompt"
  - name: p2
    text: "second prompt"
providers:
  - name: openai
    model: mock-model
  - name: anthropic
    model: mock-model
"#;

    fn make_registry(
        openai_responses: Vec<&str>,
        anthropic_responses: Vec<&str>,
    ) -> ProviderRegistry {
        let mut openai = MockProvider::new(ProviderName::Openai).accept_model("mock-model");
        for r in openai_responses {
            openai = openai.queue_response(r);
        }
        let mut anthropic = MockProvider::new(ProviderName::Anthropic).accept_model("mock-model");
        for r in anthropic_responses {
            anthropic = anthropic.queue_response(r);
        }

        let mut registry: ProviderRegistry = HashMap::new();
        registry.insert(ProviderName::Openai, Arc::new(openai));
        registry.insert(ProviderName::Anthropic, Arc::new(anthropic));
        registry
    }

    #[tokio::test]
    async fn full_matrix_runs_one_record_per_cell() {
        let cfg = Config::from_yaml_str(YAML).unwrap();
        let registry = make_registry(
            vec!["ok-openai-1", "ok-openai-2"],
            vec!["ok-ant-1", "ok-ant-2"],
        );
        let orch = Orchestrator::new(cfg, registry);
        let records = orch.run_all(OrchestratorFilter::default()).await;
        assert_eq!(records.len(), 4, "2 prompts × 2 providers");
        let ok = records
            .iter()
            .filter(|r| r.status == PromptRunStatus::Ok)
            .count();
        assert_eq!(ok, 4, "all should succeed: {records:?}");
    }

    const OPENROUTER_MODELS_YAML: &str = r#"
schema_version: '0.2'
brand:
  name: Acme
prompts:
  - name: discovery
    text: best tools?
providers:
  - name: openrouter
    models: [openai/gpt-4o, anthropic/claude-3.5-sonnet]
"#;

    #[tokio::test]
    async fn openrouter_models_list_fans_out_one_run_per_upstream() {
        let cfg = Config::from_yaml_str(OPENROUTER_MODELS_YAML).unwrap();
        let openrouter = MockProvider::new(ProviderName::Openrouter)
            .accept_model("openai/gpt-4o")
            .accept_model("anthropic/claude-3.5-sonnet")
            .queue_response("ok-1")
            .queue_response("ok-2");
        let mut registry: ProviderRegistry = HashMap::new();
        registry.insert(ProviderName::Openrouter, Arc::new(openrouter));

        let orch = Orchestrator::new(cfg, registry);
        let records = orch.run_all(OrchestratorFilter::default()).await;
        // 1 prompt × 2 upstream models = 2 runs, all under the openrouter identity.
        assert_eq!(records.len(), 2, "{records:?}");
        assert!(
            records.iter().all(|r| r.status == PromptRunStatus::Ok),
            "{records:?}"
        );
        assert!(records
            .iter()
            .all(|r| r.provider == ProviderName::Openrouter));
    }

    #[tokio::test]
    async fn openrouter_models_thread_each_upstream_model() {
        // With no client registered, the orchestrator synthesises one failed
        // record per (prompt, provider, model) — which threads each upstream
        // model verbatim, proving the per-model fan-out.
        let cfg = Config::from_yaml_str(OPENROUTER_MODELS_YAML).unwrap();
        let orch = Orchestrator::new(cfg, HashMap::new());
        let records = orch.run_all(OrchestratorFilter::default()).await;
        assert_eq!(records.len(), 2, "{records:?}");
        let mut models: Vec<&str> = records
            .iter()
            .map(|r| r.provider_model_version.as_str())
            .collect();
        models.sort();
        assert_eq!(models, vec!["anthropic/claude-3.5-sonnet", "openai/gpt-4o"]);
    }

    #[tokio::test]
    async fn failure_isolation_does_not_block_siblings() {
        let cfg = Config::from_yaml_str(YAML).unwrap();
        // openai's first call fails; second succeeds. anthropic always succeeds.
        let openai = MockProvider::new(ProviderName::Openai)
            .accept_model("mock-model")
            .queue_failure(ProviderError::rate_limited("429"))
            .queue_response("ok-openai-2");
        let anthropic = MockProvider::new(ProviderName::Anthropic)
            .accept_model("mock-model")
            .queue_response("ok-ant-1")
            .queue_response("ok-ant-2");

        let mut registry: ProviderRegistry = HashMap::new();
        registry.insert(ProviderName::Openai, Arc::new(openai));
        registry.insert(ProviderName::Anthropic, Arc::new(anthropic));

        let orch = Orchestrator::new(cfg, registry);
        let records = orch.run_all(OrchestratorFilter::default()).await;
        let summary = RunSummary::from_records(&records);
        assert_eq!(summary.total, 4);
        assert_eq!(summary.succeeded, 3);
        assert_eq!(summary.failed, 1);

        let failed = records
            .iter()
            .find(|r| r.status == PromptRunStatus::Failed)
            .unwrap();
        assert_eq!(
            failed.error_kind,
            Some(ProviderErrorKind::ProviderRateLimited)
        );
    }

    #[tokio::test]
    async fn filter_prompt_name_restricts_matrix() {
        let cfg = Config::from_yaml_str(YAML).unwrap();
        let registry = make_registry(vec!["ok-openai-1"], vec!["ok-ant-1"]);
        let orch = Orchestrator::new(cfg, registry);
        let filter = OrchestratorFilter {
            prompt_names: Some(vec!["p1".into()]),
            providers: None,
        };
        let records = orch.run_all(filter).await;
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.prompt_name == "p1"));
    }

    #[tokio::test]
    async fn filter_provider_restricts_matrix() {
        let cfg = Config::from_yaml_str(YAML).unwrap();
        let registry = make_registry(vec!["a", "b"], vec!["c", "d"]);
        let orch = Orchestrator::new(cfg, registry);
        let filter = OrchestratorFilter {
            prompt_names: None,
            providers: Some(vec![ProviderName::Openai]),
        };
        let records = orch.run_all(filter).await;
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.provider == ProviderName::Openai));
    }

    #[tokio::test]
    async fn unregistered_provider_yields_failed_record() {
        let cfg = Config::from_yaml_str(YAML).unwrap();
        // Only register openai. anthropic stays unregistered.
        let openai = MockProvider::new(ProviderName::Openai)
            .accept_model("mock-model")
            .queue_response("a")
            .queue_response("b");
        let mut registry: ProviderRegistry = HashMap::new();
        registry.insert(ProviderName::Openai, Arc::new(openai));

        let orch = Orchestrator::new(cfg, registry);
        let records = orch.run_all(OrchestratorFilter::default()).await;
        let by_provider: HashMap<ProviderName, Vec<&PromptRunRecord>> =
            records.iter().fold(HashMap::new(), |mut acc, r| {
                acc.entry(r.provider.clone()).or_default().push(r);
                acc
            });
        for rec in by_provider.get(&ProviderName::Anthropic).unwrap() {
            assert_eq!(rec.status, PromptRunStatus::Failed);
            assert!(rec
                .error_message
                .as_ref()
                .unwrap()
                .contains("ogeo login anthropic"));
        }
    }
}
