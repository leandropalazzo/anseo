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
pub mod egress;
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

/// Render a `reqwest::Error` with its full source chain.
///
/// `reqwest::Error`'s `Display` stops at the top frame — a transport failure
/// renders as a bare `error sending request for url (...)`, which names the URL
/// and nothing about *why*. The cause (DNS failure, refused connection, an
/// untrusted certificate) only lives in `source()`, so an operator seeing just
/// the top frame cannot tell a bad proxy from an intercepted TLS handshake.
/// Walk the chain so the reason travels with the error.
pub(crate) fn describe_reqwest_err(err: &reqwest::Error) -> String {
    use std::error::Error as _;

    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        // Each layer often re-wraps the layer below verbatim; skip frames that
        // add no new words rather than printing the same sentence three times.
        if !out.contains(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        source = cause.source();
    }
    out
}

/// Map a `reqwest::Error` to the closed Phase 1 taxonomy.
pub fn map_reqwest_err(err: reqwest::Error) -> ProviderError {
    let detail = describe_reqwest_err(&err);
    if err.is_timeout() {
        ProviderError::timeout(detail)
    } else if err.is_connect() || err.is_request() {
        ProviderError::network(detail)
    } else if let Some(status) = err.status() {
        match status.as_u16() {
            401 | 403 => ProviderError::unauthorized(detail),
            429 => ProviderError::rate_limited(detail),
            500..=599 => ProviderError::five_xx(detail),
            _ => ProviderError::invalid_response(detail),
        }
    } else {
        ProviderError::network(detail)
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
            .user_agent("anseo/0.1 (+https://anseo.ai)")
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

    pub fn validate_endpoint(&self, provider: &ProviderName) -> Result<(), ProviderError> {
        egress::validate_provider_base_url(provider, &self.base_url)
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

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn error_constructors_set_the_right_taxonomy_kind() {
        // Each helper must map to its dedicated closed-taxonomy variant — the
        // orchestrator persists `kind` to the `error_kind` column, so a wrong
        // mapping silently mislabels failures.
        assert_eq!(
            ProviderError::unauthorized("x").kind,
            ProviderErrorKind::ProviderUnauthorized
        );
        assert_eq!(
            ProviderError::rate_limited("x").kind,
            ProviderErrorKind::ProviderRateLimited
        );
        assert_eq!(
            ProviderError::timeout("x").kind,
            ProviderErrorKind::ProviderTimeout
        );
        assert_eq!(
            ProviderError::five_xx("x").kind,
            ProviderErrorKind::Provider5xx
        );
        assert_eq!(
            ProviderError::invalid_response("x").kind,
            ProviderErrorKind::ProviderInvalidResponse
        );
        assert_eq!(
            ProviderError::network("x").kind,
            ProviderErrorKind::NetworkError
        );
        assert_eq!(
            ProviderError::unsupported_model("x").kind,
            ProviderErrorKind::ProviderUnsupportedModel
        );
    }

    #[test]
    fn error_display_includes_kind_and_message() {
        // `#[error("{kind}: {message}")]` — both halves must surface so logs are
        // greppable by kind AND carry the upstream detail.
        let err = ProviderError::rate_limited("slow down please");
        let rendered = err.to_string();
        assert!(rendered.contains("slow down please"), "{rendered}");
        // The kind's Display is the leading segment.
        assert!(
            rendered.starts_with(&ProviderErrorKind::ProviderRateLimited.to_string()),
            "{rendered}"
        );
    }

    #[test]
    fn request_new_sets_sane_defaults() {
        let req = ProviderRequest::new("hello", "gpt-4o");
        assert_eq!(req.prompt_text, "hello");
        assert_eq!(req.model, "gpt-4o");
        // Default timeout is 60s and parameters default to an empty object.
        assert_eq!(req.timeout, Duration::from_secs(60));
        assert_eq!(req.request_parameters, serde_json::json!({}));
    }

    #[test]
    fn request_builders_override_defaults() {
        let params = serde_json::json!({ "temperature": 0, "max_tokens": 256 });
        let req = ProviderRequest::new("hi", "m")
            .with_parameters(params.clone())
            .with_timeout(Duration::from_millis(250));
        assert_eq!(req.request_parameters, params);
        assert_eq!(req.timeout, Duration::from_millis(250));
        // Unrelated fields are untouched by the builders.
        assert_eq!(req.prompt_text, "hi");
        assert_eq!(req.model, "m");
    }

    #[test]
    fn http_client_exposes_config_and_redacts_secret_in_debug() {
        let client = HttpClient::new(Secret::new("sk-super-secret"), "https://api.example.com");
        assert_eq!(client.base_url(), "https://api.example.com");
        assert_eq!(client.api_key().expose(), "sk-super-secret");
        // Debug must NOT leak the API key (it's omitted from the struct entirely).
        let dbg = format!("{client:?}");
        assert!(!dbg.contains("sk-super-secret"), "secret leaked: {dbg}");
        assert!(dbg.contains("api.example.com"), "base_url missing: {dbg}");
    }

    /// A transport failure must carry WHY it failed, not just the URL.
    /// `reqwest::Error`'s own `Display` stops at `error sending request for
    /// url (...)`, which cannot distinguish a refused connection from an
    /// untrusted certificate — the two have very different fixes.
    #[tokio::test]
    async fn network_error_message_carries_the_underlying_cause() {
        // Bind then drop, so the port is closed but reserved to nobody: a
        // connect there is refused deterministically, with no network access.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let err = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .await
            .expect_err("connecting to a closed port must fail");

        let top = err.to_string();
        let full = describe_reqwest_err(&err);

        assert!(
            full.len() > top.len(),
            "expected the source chain to add detail beyond `{top}`, got `{full}`"
        );
        assert!(
            full.starts_with(&top),
            "rendered error must keep the top frame first: `{full}`"
        );

        // And the taxonomy mapping must carry that detail through, not re-render
        // the bare top frame.
        let mapped = map_reqwest_err(err);
        assert_eq!(mapped.kind, ProviderErrorKind::NetworkError);
        assert_eq!(mapped.message, full);
    }
}
