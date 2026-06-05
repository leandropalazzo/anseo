//! Adapter that exposes a loaded plugin Provider through the first-party
//! [`Provider`] trait (Story 17.6, FR-52).
//!
//! The orchestrator and persistence layers know nothing about plugins: a
//! `PluginProvider` is registered in the [`crate::ProviderRegistry`] under a
//! [`ProviderName::Plugin`] key exactly like a first-party adapter, so a
//! Prompt Run issued against `provider: "plugin:<id>"` produces a
//! `PromptRunRecord` whose shape is indistinguishable from a first-party one.
//!
//! Phase 3 ships the in-process passthrough: the plugin's responses are
//! supplied to the adapter at construction (the SDK host wires real plugin
//! invocation in a later phase). The seam — registry key, trait surface,
//! record shape — is what 17.6 fixes in place.

use async_trait::async_trait;
use std::sync::Mutex;

use anseo_core::ProviderName;

use crate::{Provider, ProviderError, ProviderRequest, ProviderResponse};

/// A plugin-provided Provider, registered under [`ProviderName::Plugin`].
pub struct PluginProvider {
    /// Plugin provider id (e.g. `test.mock-provider`); the registry key is
    /// `ProviderName::Plugin(id)`.
    id: String,
    accepted_models: Vec<String>,
    /// Canned responses, consumed in order — the in-process passthrough used
    /// until the SDK host wires live plugin invocation.
    queued: Mutex<Vec<Result<ProviderResponse, ProviderError>>>,
}

impl PluginProvider {
    /// Construct an adapter for plugin provider `id` (the bare id, without the
    /// `plugin:` prefix).
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            accepted_models: vec!["plugin-model".to_string()],
            queued: Mutex::new(Vec::new()),
        }
    }

    pub fn accept_model(mut self, model: impl Into<String>) -> Self {
        self.accepted_models.push(model.into());
        self
    }

    /// Queue a canned success response. The persisted row carries the same
    /// [`ProviderName::Plugin`] identity as the registry key.
    pub fn queue_response(self, text: impl Into<String>) -> Self {
        let text = text.into();
        let response = ProviderResponse {
            provider: self.name(),
            model: "plugin-model".to_string(),
            region: None,
            raw_response: serde_json::json!({"plugin": self.id, "message": text}),
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
impl Provider for PluginProvider {
    fn name(&self) -> ProviderName {
        ProviderName::Plugin(self.id.clone())
    }

    fn validate_model(&self, model: &str) -> Result<String, ProviderError> {
        if self.accepted_models.iter().any(|m| m == model) {
            Ok(model.to_string())
        } else {
            Err(ProviderError::unsupported_model(format!(
                "plugin provider `{}` does not accept model `{model}`",
                self.id
            )))
        }
    }

    async fn run(&self, _request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let mut q = self.queued.lock().unwrap();
        if q.is_empty() {
            return Err(ProviderError::invalid_response(
                "plugin provider has no queued responses",
            ));
        }
        q.remove(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plugin_provider_identity_uses_plugin_variant() {
        let p = PluginProvider::new("test.mock-provider");
        assert_eq!(p.name(), ProviderName::Plugin("test.mock-provider".into()));
        assert_eq!(p.name().as_wire_str(), "plugin:test.mock-provider");
    }

    #[tokio::test]
    async fn plugin_provider_returns_queued_response() {
        let p = PluginProvider::new("test.mock-provider")
            .accept_model("plugin-model")
            .queue_response("hello from plugin");
        let req = ProviderRequest::new("anything", "plugin-model");
        let resp = p.run(req).await.unwrap();
        assert_eq!(resp.message_text, "hello from plugin");
        assert_eq!(
            resp.provider,
            ProviderName::Plugin("test.mock-provider".into())
        );
    }
}
