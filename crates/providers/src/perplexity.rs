//! Perplexity provider adapter (FR-7 / FR-25 — Phase 2 Story 11.1).
//!
//! Perplexity exposes an OpenAI-compatible `POST /chat/completions` endpoint
//! at `api.perplexity.ai`. Response shape mirrors OpenAI; the salient
//! difference is the model list (Sonar variants) and that responses include
//! a `citations` array we surface in `raw_response` for the citation
//! extractor downstream.

use async_trait::async_trait;

use opengeo_core::{ProviderName, Secret, DEFAULT_PERPLEXITY_MODEL};

use crate::{
    map_reqwest_err, HttpClient, Provider, ProviderError, ProviderRequest, ProviderResponse,
};

pub const DEFAULT_PERPLEXITY_BASE_URL: &str = "https://api.perplexity.ai";

pub const SUPPORTED_MODELS: &[&str] = &[
    DEFAULT_PERPLEXITY_MODEL,
    "sonar-large-online-128k",
    "sonar-small-online-128k",
    "sonar-large-chat-128k",
    "sonar-small-chat-128k",
    "llama-3.1-sonar-large-128k-online",
    "llama-3.1-sonar-small-128k-online",
];

pub struct PerplexityProvider {
    http: HttpClient,
}

impl PerplexityProvider {
    pub fn new(api_key: Secret) -> Self {
        Self::with_base_url(api_key, DEFAULT_PERPLEXITY_BASE_URL)
    }

    pub fn with_base_url(api_key: Secret, base_url: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(api_key, base_url),
        }
    }
}

#[async_trait]
impl Provider for PerplexityProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Perplexity
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        if SUPPORTED_MODELS.contains(&model) {
            Ok(model.to_string())
        } else {
            Err(ProviderError::unsupported_model(format!(
                "unsupported Perplexity model `{model}` (supported: {})",
                SUPPORTED_MODELS.join(", ")
            )))
        }
    }

    async fn run(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.http.base_url());
        let body = build_chat_body(&request);

        let response = self
            .http
            .inner()
            .post(&url)
            .bearer_auth(self.http.api_key().expose())
            .header("X-OpenGEO-Request-Id", request.request_id.to_string())
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

        let raw: serde_json::Value = serde_json::from_str(&raw_text).map_err(|e| {
            ProviderError::invalid_response(format!("non-JSON Perplexity response: {e}"))
        })?;

        let message_text = extract_message_text(&raw).ok_or_else(|| {
            ProviderError::invalid_response(
                "Perplexity response missing choices[0].message.content",
            )
        })?;

        let model = raw
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| request.model.clone());

        Ok(ProviderResponse {
            provider: ProviderName::Perplexity,
            model,
            region: None,
            raw_response: raw,
            message_text,
        })
    }
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
        serde_json::json!([
            {"role": "user", "content": request.prompt_text}
        ]),
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
    use opengeo_core::ProviderErrorKind;

    #[test]
    fn validate_model_accepts_supported() {
        let p = PerplexityProvider::new(Secret::new("test"));
        assert!(p
            .validate_model("llama-3.1-sonar-large-128k-online")
            .is_ok());
    }

    #[test]
    fn validate_model_rejects_unknown_with_unsupported_model_kind() {
        let p = PerplexityProvider::new(Secret::new("test"));
        let err = p.validate_model("sonar-bogus").unwrap_err();
        assert_eq!(err.kind, ProviderErrorKind::ProviderUnsupportedModel);
        assert!(err.message.contains("sonar-bogus"));
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
    fn classify_status_maps_401_to_unauthorized() {
        assert_eq!(
            classify_status(401, "auth fail").kind,
            ProviderErrorKind::ProviderUnauthorized
        );
    }

    #[test]
    fn classify_status_maps_429() {
        assert_eq!(
            classify_status(429, "slow").kind,
            ProviderErrorKind::ProviderRateLimited
        );
    }
}
