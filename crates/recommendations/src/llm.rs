//! Story 19.3 — the LLM-aided / hybrid lane (architecture §3.3–3.5, §4.4).
//!
//! The deterministic engine stays pure (AD-Phase3-RecommendationsInProcess); the
//! LLM lane is layered on top via a **synchronous** [`EnrichmentProvider`]
//! abstraction. Keeping the trait sync lets the whole crate stay offline- and
//! property-testable (the `[rec-1]` byte-stable contract for the deterministic
//! lane is unaffected). The API/CLI consumer (Story 19.6) adapts the async
//! Phase 2 `Provider` trait onto this synchronous boundary.

use serde::{Deserialize, Serialize};

/// Per-run LLM configuration + cost caps (architecture §4.4).
#[derive(Debug, Clone)]
pub struct LlmConfig {
    /// `recommendations.llm_enrich` (anseo.yaml v0.2). Default `false`.
    pub enrich: bool,
    /// ≤ 20 LLM-aided recs / run (architecture §4.4 cost cap).
    pub max_recs_per_run: usize,
    /// ≤ 8k prompt tokens / call.
    pub max_prompt_tokens: u32,
    /// ≤ 1k completion tokens / call.
    pub max_completion_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            enrich: false,
            max_recs_per_run: 20,
            max_prompt_tokens: 8_000,
            max_completion_tokens: 1_000,
        }
    }
}

/// What the lane hands the provider for one enrichment call.
#[derive(Debug, Clone)]
pub struct EnrichmentRequest {
    /// The fully-rendered prompt (brand/docs config only — never raw user
    /// Prompt text unless `llm_enrich` opt-in; §9.4 privacy boundary).
    pub full_prompt: String,
    /// Pinned to 0 for the determinism allow-list.
    pub temperature: f32,
    /// Supplied for allow-listed providers; `None` otherwise.
    pub seed: Option<i64>,
    pub max_completion_tokens: u32,
}

/// What a provider returns for one enrichment call.
#[derive(Debug, Clone)]
pub struct EnrichmentOutcome {
    pub full_response: String,
    /// FR-60: content-hash of the loaded model, when the endpoint exposes it.
    /// Absence lowers reproducibility to `non_deterministic_pipeline`.
    pub model_content_hash: Option<String>,
    pub latency_ms: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum EnrichmentError {
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("provider call failed: {0}")]
    CallFailed(String),
}

/// Synchronous enrichment boundary. The real adapter (Story 19.6) wraps the
/// async Phase 2 `Provider`; tests use [`StubProvider`].
pub trait EnrichmentProvider {
    /// Stable provider identity, e.g. `openai` or `local-oss:llm.lan`
    /// (port stripped per §3.4).
    fn provider_id(&self) -> String;
    /// Provider-specific model id, e.g. `gpt-4o-2024-08-06`.
    fn model_id(&self) -> &str;
    /// Template id + version recorded in the LlmTrace.
    fn template(&self) -> (&str, &str);
    fn enrich(&self, req: &EnrichmentRequest) -> Result<EnrichmentOutcome, EnrichmentError>;
}

/// `deterministic_providers` allow-list (architecture §3.5). Returns `true`
/// when the engine must NOT tag `non_deterministic_pipeline`: a supported
/// (provider, model) with `temperature == 0` + a `seed`, or a local-OSS
/// endpoint that supplied a `model_content_hash`.
pub fn is_deterministic_provider(
    provider_id: &str,
    model_id: &str,
    has_content_hash: bool,
    temperature: f32,
    seed: Option<i64>,
) -> bool {
    if temperature != 0.0 || seed.is_none() {
        // OpenAI allow-list requires temp=0 + seed; local-oss requires a hash
        // but the seed/temp discipline still applies.
        if !(provider_id.starts_with("local-oss:") && has_content_hash) {
            return false;
        }
    }
    if provider_id.starts_with("local-oss:") {
        return has_content_hash;
    }
    matches!(
        (provider_id, model_id),
        ("openai", "gpt-4o-2024-08-06") | ("openai", "gpt-4o-mini-2024-07-18")
    )
}

/// A top-level engine warning surfaced in the REST `warnings: []` array and on
/// CLI stderr (architecture §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineWarning {
    pub kind: String,
    pub reason: String,
    pub affected_kinds: Vec<String>,
    pub engine_version: String,
}

impl EngineWarning {
    pub fn llm_enrichment_skipped(engine_version: &str) -> Self {
        Self {
            kind: "llm_enrichment_skipped".to_string(),
            reason: "no_provider_configured".to_string(),
            affected_kinds: vec![
                "structural_content_suggestion".to_string(),
                "citation_quality_uplift".to_string(),
                "volatility_anomaly_explained".to_string(),
            ],
            engine_version: engine_version.to_string(),
        }
    }
}

/// An in-process stub provider for tests. Echoes a canned response and lets
/// tests pin the provider identity + content-hash presence to exercise the
/// allow-list + FR-60 paths.
#[derive(Debug, Clone)]
pub struct StubProvider {
    pub provider_id: String,
    pub model_id: String,
    pub model_content_hash: Option<String>,
    pub response: String,
}

impl StubProvider {
    /// An allow-listed deterministic provider (`openai:gpt-4o-2024-08-06`).
    pub fn deterministic_openai() -> Self {
        Self {
            provider_id: "openai".into(),
            model_id: "gpt-4o-2024-08-06".into(),
            model_content_hash: None,
            response: "Add an FAQ schema block and cite your docs domain.".into(),
        }
    }

    /// A non-allow-listed provider (Anthropic — no seed support).
    pub fn non_deterministic_anthropic() -> Self {
        Self {
            provider_id: "anthropic".into(),
            model_id: "claude-opus-4-7".into(),
            model_content_hash: None,
            response: "Consider restructuring your landing copy.".into(),
        }
    }

    /// A local-OSS endpoint; `with_hash` toggles the FR-60 content-hash path.
    pub fn local_oss(with_hash: bool) -> Self {
        Self {
            provider_id: "local-oss:llm.lan".into(),
            model_id: "llama-3.1-70b".into(),
            model_content_hash: with_hash.then(|| "sha256:modelhash".to_string()),
            response: "Tighten your structured data.".into(),
        }
    }
}

impl EnrichmentProvider for StubProvider {
    fn provider_id(&self) -> String {
        self.provider_id.clone()
    }
    fn model_id(&self) -> &str {
        &self.model_id
    }
    fn template(&self) -> (&str, &str) {
        ("rec-enrich-v1", "1.0.0")
    }
    fn enrich(&self, _req: &EnrichmentRequest) -> Result<EnrichmentOutcome, EnrichmentError> {
        Ok(EnrichmentOutcome {
            full_response: self.response.clone(),
            model_content_hash: self.model_content_hash.clone(),
            latency_ms: 12,
            tokens_in: 128,
            tokens_out: 64,
        })
    }
}
