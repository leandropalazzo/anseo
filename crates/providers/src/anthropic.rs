//! Anthropic provider adapter (FR-8).
//!
//! Hits `POST /v1/messages` with `x-api-key` + `anthropic-version` headers.
//! Same error-taxonomy mapping as the OpenAI adapter.

use async_trait::async_trait;

use anseo_core::{ProviderName, Secret};

use crate::{
    map_reqwest_err, HttpClient, Provider, ProviderError, ProviderRequest, ProviderResponse,
};

pub const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

pub const SUPPORTED_MODELS: &[&str] = &[
    "claude-3-5-sonnet-20241022",
    "claude-3-5-haiku-20241022",
    "claude-3-opus-20240229",
    "claude-3-sonnet-20240229",
    "claude-3-haiku-20240307",
];

pub struct AnthropicProvider {
    http: HttpClient,
}

impl AnthropicProvider {
    pub fn new(api_key: Secret) -> Self {
        Self::with_base_url(api_key, DEFAULT_ANTHROPIC_BASE_URL)
    }

    pub fn with_base_url(api_key: Secret, base_url: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(api_key, base_url),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Anthropic
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        if SUPPORTED_MODELS.contains(&model) {
            Ok(model.to_string())
        } else {
            Err(ProviderError::invalid_response(format!(
                "unsupported Anthropic model `{model}` (supported: {})",
                SUPPORTED_MODELS.join(", ")
            )))
        }
    }

    async fn run(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let url = format!("{}/v1/messages", self.http.base_url());
        let body = build_messages_body(&request);

        let response = self
            .http
            .inner()
            .post(&url)
            .header("x-api-key", self.http.api_key().expose())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("X-Anseo-Request-Id", request.request_id.to_string())
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
            ProviderError::invalid_response(format!("non-JSON Anthropic response: {e}"))
        })?;

        let message_text = extract_message_text(&raw).ok_or_else(|| {
            ProviderError::invalid_response(
                "Anthropic response missing content[*].text or content[*].type=text",
            )
        })?;

        let model = raw
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| request.model.clone());

        Ok(ProviderResponse {
            provider: ProviderName::Anthropic,
            model,
            region: None,
            raw_response: raw,
            message_text,
        })
    }
}

fn build_messages_body(request: &ProviderRequest) -> serde_json::Value {
    let mut body = match &request.request_parameters {
        serde_json::Value::Object(map) => serde_json::Value::Object(map.clone()),
        _ => serde_json::json!({}),
    };
    let obj = body.as_object_mut().expect("seeded as Object");
    obj.insert(
        "model".into(),
        serde_json::Value::String(request.model.clone()),
    );
    obj.entry("max_tokens")
        .or_insert(serde_json::Value::from(1024));
    obj.insert(
        "messages".into(),
        serde_json::json!([
            {"role": "user", "content": request.prompt_text}
        ]),
    );
    body
}

fn extract_message_text(raw: &serde_json::Value) -> Option<String> {
    // Anthropic responses have `content: [{type: "text", text: "…"}]`.
    let arr = raw.get("content")?.as_array()?;
    let mut out = String::new();
    for block in arr {
        let kind = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if kind != "text" {
            continue;
        }
        if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(t);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
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
    fn validate_model_accepts_supported() {
        let p = AnthropicProvider::new(Secret::new("test"));
        assert!(p.validate_model("claude-3-5-sonnet-20241022").is_ok());
    }

    #[test]
    fn validate_model_rejects_unknown() {
        let p = AnthropicProvider::new(Secret::new("test"));
        let err = p.validate_model("claude-99").unwrap_err();
        assert_eq!(err.kind, ProviderErrorKind::ProviderInvalidResponse);
    }

    #[test]
    fn extract_concatenates_text_blocks() {
        let raw = serde_json::json!({
            "content": [
                {"type": "text", "text": "alpha"},
                {"type": "tool_use", "name": "x"},
                {"type": "text", "text": "beta"}
            ]
        });
        assert_eq!(extract_message_text(&raw).as_deref(), Some("alpha\nbeta"));
    }

    #[test]
    fn classify_status_maps_429() {
        assert_eq!(
            classify_status(429, "slow").kind,
            ProviderErrorKind::ProviderRateLimited
        );
    }
}
