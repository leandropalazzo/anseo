//! Provider abstractions and Phase 1 LLM adapters (FR-7, FR-8, FR-9, FR-2).
//!
//! `crates/providers` owns:
//! - The [`Provider`] trait — the contract every LLM adapter implements so
//!   `apps/cli` and `apps/worker` can issue Prompt Runs without knowing whose
//!   API they're hitting.
//! - Phase 1 adapters: [`OpenAiProvider`], [`AnthropicProvider`]. Both use
//!   `reqwest` with a per-call timeout and map every failure mode into the
//!   closed [`anseo_core::ProviderErrorKind`] taxonomy.
//! - A pre-flight model allowlist used by `Provider::validate_model` so the
//!   CLI rejects unsupported model names *before* making any API call.
//! - A [`MockProvider`] for tests/CI — canned responses, configurable
//!   failures, no network.
//!
//! No HTTP details leak through the `Provider` trait: callers see only
//! [`ProviderRequest`] / [`ProviderResponse`] / [`ProviderError`].

pub mod anthropic;
pub mod cost;
pub mod gemini;
pub mod grok;
pub mod mistral;
pub mod mock;
pub mod openai;
pub mod openrouter;
pub mod orchestrator;
pub mod perplexity;
pub mod persistence;
pub mod plugin;
pub mod registry;

use async_trait::async_trait;
use std::time::Duration;

use anseo_core::{ProviderErrorKind, ProviderName, RequestId, Secret};

/// What the orchestrator sends to a provider for a single Prompt Run.
#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub prompt_text: String,
    /// Provider-specific model identifier. Already validated by
    /// [`Provider::validate_model`] before construction.
    pub model: String,
    /// Optional sampling parameters (temperature, max_tokens, top_p, …). Stored
    /// as opaque JSON so the schema remains stable across provider quirks.
    pub request_parameters: serde_json::Value,
    pub timeout: Duration,
    /// Correlation ID. Threaded into HTTP headers as `X-Anseo-Request-Id`.
    pub request_id: RequestId,
}

impl ProviderRequest {
    pub fn new(prompt_text: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            prompt_text: prompt_text.into(),
            model: model.into(),
            request_parameters: serde_json::json!({}),
            timeout: Duration::from_secs(60),
            request_id: RequestId::new(),
        }
    }

    pub fn with_parameters(mut self, params: serde_json::Value) -> Self {
        self.request_parameters = params;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// What a provider returns on success.
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub provider: ProviderName,
    pub model: String,
    /// Optional region hint (e.g. AWS region for Anthropic Bedrock proxy).
    /// `None` for the direct provider APIs we hit in Phase 1.
    pub region: Option<String>,
    /// Full raw response body, JSON-shaped. Persisted verbatim per NFR-1.
    pub raw_response: serde_json::Value,
    /// Best-effort flattened "what the model said" string used by extractors.
    /// Construction is provider-specific; see each adapter.
    pub message_text: String,
}

/// Structured provider failure. Maps cleanly into the
/// [`ProviderErrorKind`] taxonomy and into `error_kind` column rows.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{kind}: {message}")]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::ProviderUnauthorized, message)
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::ProviderRateLimited, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::ProviderTimeout, message)
    }

    pub fn five_xx(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Provider5xx, message)
    }

    pub fn invalid_response(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::ProviderInvalidResponse, message)
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::NetworkError, message)
    }

    /// Story 11.1: distinct "you asked for a model the adapter doesn't
    /// know" failure. Use instead of `invalid_response` for typo'd model
    /// strings so the operator can grep their config rather than
    /// hunting an opaque API response.
    pub fn unsupported_model(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::ProviderUnsupportedModel, message)
    }
}

/// Map a `reqwest::Error` to the closed Phase 1 taxonomy.
pub fn map_reqwest_err(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        ProviderError::timeout(err.to_string())
    } else if err.is_connect() || err.is_request() {
        ProviderError::network(err.to_string())
    } else if let Some(status) = err.status() {
        match status.as_u16() {
            401 | 403 => ProviderError::unauthorized(err.to_string()),
            429 => ProviderError::rate_limited(err.to_string()),
            500..=599 => ProviderError::five_xx(err.to_string()),
            _ => ProviderError::invalid_response(err.to_string()),
        }
    } else {
        ProviderError::network(err.to_string())
    }
}

/// Provider abstraction. Each adapter is a thin async wrapper around the
/// remote API.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Identity. Always matches the corresponding [`ProviderName`] variant.
    fn name(&self) -> ProviderName;

    /// Pre-flight check before the orchestrator builds a [`ProviderRequest`].
    /// Returns the canonical model string the provider expects. Implementations
    /// must reject unsupported models with [`ProviderErrorKind::ProviderInvalidResponse`]
    /// so the orchestrator can short-circuit before the network call (FR-9 AC).
    fn validate_model(&self, model: &str) -> Result<String, ProviderError>;

    /// Issue a Prompt Run. The future MUST honor `request.timeout`.
    async fn run(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError>;
}

/// Shared HTTP client configuration. Both Phase 1 adapters consume this.
#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    api_key: Secret,
    base_url: String,
}

impl HttpClient {
    pub fn new(api_key: Secret, base_url: impl Into<String>) -> Self {
        let inner = reqwest::Client::builder()
            // Per-request timeouts are applied via `Request::with_timeout`;
            // this is an upper bound across pool reuse / DNS resolution.
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .user_agent("opengeo/0.1 (+https://github.com/anthropics)")
            .build()
            .expect("reqwest client builder always succeeds with default config");
        Self {
            inner,
            api_key,
            base_url: base_url.into(),
        }
    }

    pub fn inner(&self) -> &reqwest::Client {
        &self.inner
    }

    pub fn api_key(&self) -> &Secret {
        &self.api_key
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Intentionally omit the Secret. Even though `Secret`'s own Debug
        // redacts, omitting is one fewer chance for a future change to leak.
        f.debug_struct("HttpClient")
            .field("base_url", &self.base_url)
            .finish()
    }
}

pub use anthropic::AnthropicProvider;
pub use mock::MockProvider;
pub use openai::OpenAiProvider;
pub use orchestrator::{
    Orchestrator, OrchestratorFilter, PromptRunRecord, PromptRunStatus, ProviderRegistry,
    RunSummary,
};
pub use plugin::PluginProvider;
