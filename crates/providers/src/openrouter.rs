//! OpenRouter aggregator adapter — Phase 2 Story 11.3.
//!
//! OpenRouter routes a single API to many upstreams (OpenAI, Anthropic,
//! Mistral, etc.) using model identifiers of the form `<vendor>/<model>`
//! (e.g., `openai/gpt-4o-2024-08-06`). The wire shape is OpenAI-compatible.
//!
//! Distinct from the per-vendor adapters in two ways:
//!
//! 1. The model allowlist is intentionally NOT exhaustive — OpenRouter
//!    routinely adds models. We accept any `<vendor>/<model>` shape (slash
//!    in the name) and reject anything else as `unsupported_model`. This is
//!    a structural check, not a closed list.
//! 2. The response includes the upstream model in `model` (e.g., echoes
//!    `openai/gpt-4o-2024-08-06` even if we requested `openai/gpt-4o`).
//!    We record that upstream model into a top-level `upstream_model` field
//!    in `raw_response.metadata` so the persistence layer can thread it into
//!    `prompt_runs.metadata` per the Story 11.3 wire contract.

use async_trait::async_trait;

use anseo_core::{ProviderName, Secret, DEFAULT_OPENROUTER_MODEL};

use crate::{
    map_reqwest_err, HttpClient, Provider, ProviderError, ProviderRequest, ProviderResponse,
};

pub const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api";

pub struct OpenRouterProvider {
    http: HttpClient,
}

impl OpenRouterProvider {
    pub fn new(api_key: Secret) -> Self {
        Self::with_base_url(api_key, DEFAULT_OPENROUTER_BASE_URL)
    }

    pub fn with_base_url(api_key: Secret, base_url: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(api_key, base_url),
        }
    }
}

/// OpenRouter-backed adapter registered under a concrete provider identity.
///
/// This lets operators store one OpenRouter key while the data model continues
/// to record concrete providers (`gemini`, `perplexity`, etc.). Direct provider
/// clients are registered first; this wrapper is only used as a fallback.
pub struct RoutedOpenRouterProvider {
    target: ProviderName,
    upstream_vendor: &'static str,
    inner: OpenRouterProvider,
}

impl RoutedOpenRouterProvider {
    pub fn new(target: ProviderName, api_key: Secret) -> Option<Self> {
        let upstream_vendor = match target {
            ProviderName::Openai => "openai",
            ProviderName::Anthropic => "anthropic",
            ProviderName::Gemini => "google",
            ProviderName::Perplexity => "perplexity",
            ProviderName::Grok => "x-ai",
            ProviderName::Mistral => "mistralai",
            ProviderName::Openrouter | ProviderName::Plugin(_) => return None,
        };
        Some(Self {
            target,
            upstream_vendor,
            inner: OpenRouterProvider::new(api_key),
        })
    }

    fn upstream_model(&self, model: &str) -> String {
        if model.contains('/') {
            model.to_string()
        } else {
            format!("{}/{}", self.upstream_vendor, model)
        }
    }
}

#[async_trait]
impl Provider for RoutedOpenRouterProvider {
    fn name(&self) -> ProviderName {
        self.target.clone()
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        self.inner.validate_model(&self.upstream_model(model))
    }

    async fn run(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let mut response = self.inner.run(request).await?;
        response.provider = self.target.clone();
        Ok(response)
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Openrouter
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        // OpenRouter expects `<vendor>/<model>`. Structurally validate
        // without an exhaustive list — the aggregator adds models
        // frequently and a closed list would drift constantly.
        if is_vendor_slash_model(model) {
            Ok(model.to_string())
        } else {
            Err(ProviderError::unsupported_model(format!(
                "OpenRouter model `{model}` must be in `<vendor>/<model>` form (e.g., `{DEFAULT_OPENROUTER_MODEL}`)"
            )))
        }
    }

    async fn run(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/v1/chat/completions", self.http.base_url());
        let body = build_chat_body(&request);

        let response = self
            .http
            .inner()
            .post(&url)
            .bearer_auth(self.http.api_key().expose())
            .header("X-Anseo-Request-Id", request.request_id.to_string())
            .header("HTTP-Referer", "https://github.com/anthropics")
            .header("X-Title", "OpenGEO")
            .timeout(request.timeout)
            .json(&body)
            .send()
            .await
            .map_err(map_reqwest_err)?;

        let status = response.status();
        let raw_text = response.text().await.map_err(map_reqwest_err)?;

        if !status.is_success() {
            return Err(classify_status(status.as_u16(), &raw_text));
        }

        let mut raw: serde_json::Value = serde_json::from_str(&raw_text).map_err(|e| {
            ProviderError::invalid_response(format!("non-JSON OpenRouter response: {e}"))
        })?;

        let message_text = extract_message_text(&raw).ok_or_else(|| {
            ProviderError::invalid_response(
                "OpenRouter response missing choices[0].message.content",
            )
        })?;

        let model = raw
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| request.model.clone());

        // Thread upstream_model into the raw_response metadata so persistence
        // can store it on prompt_runs.metadata.upstream_model per Story 11.3.
        if let Some(obj) = raw.as_object_mut() {
            let meta = obj
                .entry("metadata")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(meta_obj) = meta.as_object_mut() {
                meta_obj.insert(
                    "upstream_model".into(),
                    serde_json::Value::String(model.clone()),
                );
            }
        }

        Ok(ProviderResponse {
            provider: ProviderName::Openrouter,
            model,
            region: None,
            raw_response: raw,
            message_text,
        })
    }
}

fn is_vendor_slash_model(model: &str) -> bool {
    let mut parts = model.split('/');
    let vendor = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    // Exactly one slash, both halves non-empty and ASCII-alnum/-dot/-underscore.
    if parts.next().is_some() {
        return false;
    }
    !vendor.is_empty()
        && !rest.is_empty()
        && vendor
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn build_chat_body(request: &ProviderRequest) -> serde_json::Value {
    let mut body = match &request.request_parameters {
        serde_json::Value::Object(map) => serde_json::Value::Object(map.clone()),
        _ => serde_json::json!({}),
    };
    let obj = body.as_object_mut().expect("seeded as Object");
    obj.insert(
        "model".into(),
        serde_json::Value::String(request.model.clone()),
    );
    obj.insert(
        "messages".into(),
        serde_json::json!([{"role": "user", "content": request.prompt_text}]),
    );
    body
}

fn extract_message_text(raw: &serde_json::Value) -> Option<String> {
    raw.get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()
        .map(str::to_string)
}

fn classify_status(status: u16, body: &str) -> ProviderError {
    let truncated_body = body.chars().take(400).collect::<String>();
    match status {
        401 | 403 => ProviderError::unauthorized(format!("HTTP {status}: {truncated_body}")),
        429 => ProviderError::rate_limited(format!("HTTP {status}: {truncated_body}")),
        500..=599 => ProviderError::five_xx(format!("HTTP {status}: {truncated_body}")),
        _ => ProviderError::invalid_response(format!("HTTP {status}: {truncated_body}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_core::ProviderErrorKind;

    #[test]
    fn validate_model_accepts_vendor_slash_model() {
        let p = OpenRouterProvider::new(Secret::new("test"));
        assert!(p.validate_model("openai/gpt-4o-2024-08-06").is_ok());
        assert!(p.validate_model("anthropic/claude-3.5-sonnet").is_ok());
        assert!(p.validate_model("mistralai/mistral-large").is_ok());
    }

    #[test]
    fn validate_model_rejects_bare_name() {
        let p = OpenRouterProvider::new(Secret::new("test"));
        let err = p.validate_model("gpt-4o").unwrap_err();
        assert_eq!(err.kind, ProviderErrorKind::ProviderUnsupportedModel);
    }

    #[test]
    fn validate_model_rejects_double_slash() {
        let p = OpenRouterProvider::new(Secret::new("test"));
        let err = p.validate_model("a/b/c").unwrap_err();
        assert_eq!(err.kind, ProviderErrorKind::ProviderUnsupportedModel);
    }

    #[test]
    fn validate_model_rejects_empty_halves() {
        let p = OpenRouterProvider::new(Secret::new("test"));
        assert!(p.validate_model("/gpt-4o").is_err());
        assert!(p.validate_model("openai/").is_err());
    }

    #[test]
    fn routed_openrouter_keeps_concrete_identity_and_maps_model() {
        let p = RoutedOpenRouterProvider::new(ProviderName::Gemini, Secret::new("test"))
            .expect("gemini is routable through OpenRouter");
        assert_eq!(p.name(), ProviderName::Gemini);
        assert_eq!(
            p.validate_model("gemini-1.5-pro").unwrap(),
            "google/gemini-1.5-pro"
        );
    }

    #[test]
    fn extract_message_text_from_canonical_shape() {
        let raw = serde_json::json!({
            "choices": [
                {"message": {"role": "assistant", "content": "hello"}}
            ]
        });
        assert_eq!(extract_message_text(&raw).as_deref(), Some("hello"));
    }

    #[test]
    fn classify_status_maps_429() {
        assert_eq!(
            classify_status(429, "slow").kind,
            ProviderErrorKind::ProviderRateLimited
        );
    }
}
