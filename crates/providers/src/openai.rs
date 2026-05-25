//! OpenAI provider adapter (FR-7).
//!
//! Hits `POST /v1/chat/completions` with `Bearer` auth. Maps the response into
//! [`ProviderResponse`]; maps every failure mode into
//! [`opengeo_core::ProviderErrorKind`].

use async_trait::async_trait;
use std::time::Duration;

use opengeo_core::{ProviderName, Secret};

use crate::{
    map_reqwest_err, HttpClient, Provider, ProviderError, ProviderRequest, ProviderResponse,
};

/// Default OpenAI base URL. Overridable via `OpenAiProvider::with_base_url`
/// for tests against wiremock.
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com";

/// Allowlist of Phase 1 supported models. Pre-flight validation rejects
/// anything outside this set so we never burn API credits on a typo (FR-9 AC).
pub const SUPPORTED_MODELS: &[&str] = &[
    "gpt-4o-2024-08-06",
    "gpt-4o-mini-2024-07-18",
    "gpt-4-turbo-2024-04-09",
    "gpt-3.5-turbo-0125",
];

pub struct OpenAiProvider {
    http: HttpClient,
}

impl OpenAiProvider {
    pub fn new(api_key: Secret) -> Self {
        Self::with_base_url(api_key, DEFAULT_OPENAI_BASE_URL)
    }

    pub fn with_base_url(api_key: Secret, base_url: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(api_key, base_url),
        }
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Openai
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        if SUPPORTED_MODELS.contains(&model) {
            Ok(model.to_string())
        } else {
            Err(ProviderError::invalid_response(format!(
                "unsupported OpenAI model `{model}` (supported: {})",
                SUPPORTED_MODELS.join(", ")
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
            ProviderError::invalid_response(format!("non-JSON OpenAI response: {e}"))
        })?;

        let message_text = extract_message_text(&raw).ok_or_else(|| {
            ProviderError::invalid_response("OpenAI response missing choices[0].message.content")
        })?;

        let model = raw
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| request.model.clone());

        Ok(ProviderResponse {
            provider: ProviderName::Openai,
            model,
            region: None,
            raw_response: raw,
            message_text,
        })
    }
}

fn build_chat_body(request: &ProviderRequest) -> serde_json::Value {
    // Start from the user's request_parameters (temperature, top_p, etc) and
    // overlay model + messages so the orchestrator can't accidentally pass a
    // conflicting `model` field.
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

/// Convenience constructor used by tests that need a `ProviderRequest`
/// targeting a fast-failing remote endpoint.
#[doc(hidden)]
pub fn _doctest_ctor() -> OpenAiProvider {
    OpenAiProvider::new(Secret::new("test"))
}

#[allow(dead_code)]
const _DEFAULT_TIMEOUT_SANITY: Duration = Duration::from_secs(60);

#[cfg(test)]
mod tests {
    use super::*;
    use opengeo_core::ProviderErrorKind;

    #[test]
    fn validate_model_accepts_supported() {
        let p = OpenAiProvider::new(Secret::new("test"));
        assert!(p.validate_model("gpt-4o-2024-08-06").is_ok());
    }

    #[test]
    fn validate_model_rejects_unknown() {
        let p = OpenAiProvider::new(Secret::new("test"));
        let err = p.validate_model("gpt-77").unwrap_err();
        assert_eq!(err.kind, ProviderErrorKind::ProviderInvalidResponse);
        assert!(err.message.contains("gpt-77"));
    }

    #[test]
    fn classify_status_maps_401_to_unauthorized() {
        assert_eq!(
            classify_status(401, "auth fail").kind,
            ProviderErrorKind::ProviderUnauthorized
        );
    }

    #[test]
    fn classify_status_maps_429_to_rate_limited() {
        assert_eq!(
            classify_status(429, "slow down").kind,
            ProviderErrorKind::ProviderRateLimited
        );
    }

    #[test]
    fn classify_status_maps_5xx() {
        assert_eq!(
            classify_status(503, "down").kind,
            ProviderErrorKind::Provider5xx
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
}
