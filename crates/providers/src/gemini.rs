//! Gemini provider adapter (FR-7 / FR-25 — Phase 2 Story 11.1).
//!
//! Hits `POST /v1beta/models/{model}:generateContent?key={API_KEY}`. Gemini
//! passes the API key as a URL query parameter (not a Bearer header), so the
//! adapter constructs the URL with the key inline and never logs the URL.
//!
//! Response shape: `candidates[0].content.parts[*].text` concatenated.

use async_trait::async_trait;

use anseo_core::{ProviderName, Secret, DEFAULT_GEMINI_MODEL};

use crate::{
    map_reqwest_err, HttpClient, Provider, ProviderError, ProviderRequest, ProviderResponse,
};

pub const DEFAULT_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub const SUPPORTED_MODELS: &[&str] = &[
    DEFAULT_GEMINI_MODEL,
    "gemini-1.5-pro",
    "gemini-1.5-flash-002",
    "gemini-1.5-flash",
    "gemini-2.0-flash-exp",
];

pub struct GeminiProvider {
    http: HttpClient,
}

impl GeminiProvider {
    pub fn new(api_key: Secret) -> Self {
        Self::with_base_url(api_key, DEFAULT_GEMINI_BASE_URL)
    }

    pub fn with_base_url(api_key: Secret, base_url: impl Into<String>) -> Self {
        Self {
            http: HttpClient::new(api_key, base_url),
        }
    }
}

#[async_trait]
impl Provider for GeminiProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Gemini
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        if SUPPORTED_MODELS.contains(&model) {
            Ok(model.to_string())
        } else {
            Err(ProviderError::unsupported_model(format!(
                "unsupported Gemini model `{model}` (supported: {})",
                SUPPORTED_MODELS.join(", ")
            )))
        }
    }

    async fn run(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.http.validate_endpoint(&ProviderName::Gemini)?;
        // The key goes in the URL — never in a log line and never via
        // headers. Percent-encode it defensively: real Gemini keys are
        // alphanumeric so encoding is a no-op today, but a future key
        // format change must not produce a broken URL.
        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            self.http.base_url(),
            request.model,
            percent_encode_key(self.http.api_key().expose()),
        );
        let body = build_generate_body(&request);

        let response = self
            .http
            .inner()
            .post(&url)
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
            ProviderError::invalid_response(format!("non-JSON Gemini response: {e}"))
        })?;

        let message_text = extract_message_text(&raw).ok_or_else(|| {
            ProviderError::invalid_response(
                "Gemini response missing candidates[0].content.parts[*].text",
            )
        })?;

        // Gemini does not echo back `model` in the response top-level; the
        // request's model is canonical.
        let model = request.model.clone();

        Ok(ProviderResponse {
            provider: ProviderName::Gemini,
            model,
            region: None,
            raw_response: raw,
            message_text,
        })
    }
}

fn build_generate_body(request: &ProviderRequest) -> serde_json::Value {
    let mut body = match &request.request_parameters {
        serde_json::Value::Object(map) => serde_json::Value::Object(map.clone()),
        _ => serde_json::json!({}),
    };
    let obj = body.as_object_mut().expect("seeded as Object");
    obj.insert(
        "contents".into(),
        serde_json::json!([
            {
                "role": "user",
                "parts": [{"text": request.prompt_text}]
            }
        ]),
    );
    body
}

fn extract_message_text(raw: &serde_json::Value) -> Option<String> {
    let parts = raw
        .get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .as_array()?;
    let mut out = String::new();
    for part in parts {
        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
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

/// Minimal RFC 3986 unreserved-set encoder. Anything outside
/// `A-Z a-z 0-9 - . _ ~` becomes `%XX`. Avoids pulling a full URL crate
/// for the one byte we need it.
fn percent_encode_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn classify_status(status: u16, body: &str) -> ProviderError {
    let truncated_body = body.chars().take(400).collect::<String>();
    match status {
        // Gemini returns 400 for "API key not valid" — surface as
        // unauthorized so operators look at their key, not the model.
        400 if truncated_body.contains("API key") => {
            ProviderError::unauthorized(format!("HTTP {status}: {truncated_body}"))
        }
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
        let p = GeminiProvider::new(Secret::new("test"));
        assert!(p.validate_model("gemini-1.5-pro-002").is_ok());
    }

    #[test]
    fn validate_model_rejects_unknown_with_unsupported_model_kind() {
        // Story 11.1: typo'd model strings get the distinct
        // `provider_unsupported_model` variant, not the catch-all
        // `provider_invalid_response`.
        let p = GeminiProvider::new(Secret::new("test"));
        let err = p.validate_model("gemini-99").unwrap_err();
        assert_eq!(err.kind, ProviderErrorKind::ProviderUnsupportedModel);
        assert!(err.message.contains("gemini-99"));
    }

    #[test]
    fn extract_concatenates_text_parts() {
        let raw = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "alpha"},
                        {"text": "beta"}
                    ]
                }
            }]
        });
        assert_eq!(extract_message_text(&raw).as_deref(), Some("alpha\nbeta"));
    }

    #[test]
    fn extract_returns_none_for_missing_candidates() {
        let raw = serde_json::json!({"promptFeedback": {"blockReason": "SAFETY"}});
        assert!(extract_message_text(&raw).is_none());
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
            classify_status(500, "internal").kind,
            ProviderErrorKind::Provider5xx
        );
    }

    #[test]
    fn classify_status_maps_400_api_key_message_to_unauthorized() {
        // Gemini returns 400 for bad-key. Re-route to `unauthorized` so
        // operators read it as a config issue.
        let err = classify_status(400, "API key not valid");
        assert_eq!(err.kind, ProviderErrorKind::ProviderUnauthorized);
    }

    #[test]
    fn classify_status_400_without_api_key_message_is_invalid_response() {
        let err = classify_status(400, "Schema mismatch");
        assert_eq!(err.kind, ProviderErrorKind::ProviderInvalidResponse);
    }

    #[test]
    fn percent_encode_passes_unreserved_unchanged() {
        assert_eq!(percent_encode_key("ABCxyz019-._~"), "ABCxyz019-._~");
    }

    #[test]
    fn percent_encode_escapes_reserved_chars() {
        assert_eq!(percent_encode_key("a&b?c d"), "a%26b%3Fc%20d");
    }
}
