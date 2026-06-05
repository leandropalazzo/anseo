//! In-process mock provider used by tests, the orchestrator's red-phase
//! smoke, and the screenshot-fixture step of the manual build.
//!
//! `MockProvider` is fully deterministic: every call returns the canned
//! response queued for that `(provider, model)` pair, or, if `force_failure`
//! is set, the queued [`ProviderError`].

use async_trait::async_trait;
use std::sync::Mutex;

use anseo_core::ProviderName;

use crate::{Provider, ProviderError, ProviderRequest, ProviderResponse};

pub struct MockProvider {
    identity: ProviderName,
    accepted_models: Vec<String>,
    queued: Mutex<Vec<Result<ProviderResponse, ProviderError>>>,
}

impl MockProvider {
    pub fn new(identity: ProviderName) -> Self {
        Self {
            identity,
            accepted_models: vec!["mock-model".to_string()],
            queued: Mutex::new(Vec::new()),
        }
    }

    pub fn accept_model(mut self, model: impl Into<String>) -> Self {
        self.accepted_models.push(model.into());
        self
    }

    pub fn queue_response(self, text: impl Into<String>) -> Self {
        let text = text.into();
        let response = ProviderResponse {
            provider: self.identity.clone(),
            model: "mock-model".to_string(),
            region: None,
            raw_response: serde_json::json!({"mock": true, "message": text}),
            message_text: text,
        };
        self.queued.lock().unwrap().push(Ok(response));
        self
    }

    pub fn queue_failure(self, err: ProviderError) -> Self {
        self.queued.lock().unwrap().push(Err(err));
        self
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn name(&self) -> ProviderName {
        self.identity.clone()
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        if self.accepted_models.iter().any(|m| m == model) {
            Ok(model.to_string())
        } else {
            Err(ProviderError::invalid_response(format!(
                "MockProvider does not accept `{model}`"
            )))
        }
    }

    async fn run(&self, _request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let mut q = self.queued.lock().unwrap();
        if q.is_empty() {
            return Err(ProviderError::invalid_response(
                "MockProvider has no queued responses",
            ));
        }
        q.remove(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_returns_queued_response() {
        let p = MockProvider::new(ProviderName::Openai)
            .accept_model("gpt-4o-2024-08-06")
            .queue_response("hi from mock");
        let req = ProviderRequest::new("anything", "gpt-4o-2024-08-06");
        let resp = p.run(req).await.unwrap();
        assert_eq!(resp.message_text, "hi from mock");
        assert_eq!(resp.provider, ProviderName::Openai);
    }

    #[tokio::test]
    async fn mock_returns_queued_failure() {
        let p = MockProvider::new(ProviderName::Anthropic)
            .accept_model("claude-3-5-sonnet-20241022")
            .queue_failure(ProviderError::rate_limited("429"));
        let req = ProviderRequest::new("x", "claude-3-5-sonnet-20241022");
        let err = p.run(req).await.unwrap_err();
        assert_eq!(err.kind, anseo_core::ProviderErrorKind::ProviderRateLimited);
    }
}
