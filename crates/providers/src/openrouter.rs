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

/// OpenRouter's vendor segment for a first-party provider, or `None` for the
/// providers that cannot be routed through it.
pub fn upstream_vendor_for(target: &ProviderName) -> Option<&'static str> {
    match target {
        ProviderName::Openai => Some("openai"),
        ProviderName::Anthropic => Some("anthropic"),
        ProviderName::Gemini => Some("google"),
        ProviderName::Perplexity => Some("perplexity"),
        ProviderName::Grok => Some("x-ai"),
        ProviderName::Mistral => Some("mistralai"),
        ProviderName::Openrouter | ProviderName::Plugin(_) => None,
    }
}

/// The slug a routed provider sends when no model is configured — its vendor
/// segment joined to the provider's own default model.
///
/// This is the string that must exist in OpenRouter's catalog. It frequently
/// does not: a vendor-native id like `claude-3-5-sonnet-20241022` is not an
/// OpenRouter slug, and prefixing the vendor does not make it one.
pub fn default_routed_slug(target: &ProviderName) -> Option<String> {
    upstream_vendor_for(target)?;
    target.default_openrouter_model().map(str::to_string)
}

impl RoutedOpenRouterProvider {
    pub fn new(target: ProviderName, api_key: Secret) -> Option<Self> {
        let upstream_vendor = upstream_vendor_for(&target)?;
        Some(Self {
            target,
            upstream_vendor,
            inner: OpenRouterProvider::new(api_key),
        })
    }

    fn upstream_model(&self, model: &str) -> String {
        // Already a slug: the caller named an OpenRouter model outright.
        if model.contains('/') {
            return model.to_string();
        }
        // Nothing was configured, so the orchestrator handed us the provider's
        // NATIVE default. Prefixing the vendor onto that yields a slug
        // OpenRouter rejects, so swap in the routed default instead.
        if model == self.target.default_model() {
            if let Some(routed) = self.target.default_openrouter_model() {
                return routed.to_string();
            }
        }
        // An explicitly configured bare model name: prefix the vendor and let
        // OpenRouter judge it. Guessing further would hide the operator's
        // intent behind a default they did not ask for.
        format!("{}/{}", self.upstream_vendor, model)
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
        self.http.validate_endpoint(&ProviderName::Openrouter)?;
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

/// How long the catalog fetch may take before registration gives up on it.
/// Short on purpose: this runs on the boot path, and an unreachable
/// aggregator must not hold the process up.
const CATALOG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// The set of model slugs OpenRouter currently serves.
///
/// Registration uses this to refuse a routed provider whose slug the
/// aggregator does not list. Without it, a provider whose default model is not
/// an OpenRouter slug registers happily and then fails every single run with
/// `provider_invalid_response` — the aggregator answers HTTP 400, which is
/// indistinguishable at a glance from a malformed reply.
#[derive(Debug, Clone)]
pub struct OpenRouterCatalog {
    slugs: std::collections::HashSet<String>,
}

impl OpenRouterCatalog {
    /// Fetch the catalog from the default OpenRouter endpoint.
    ///
    /// `None` means the catalog could not be established — offline, blocked,
    /// rate-limited, or a shape we do not recognise. Callers treat that as
    /// "cannot validate" and register as before, because refusing to register
    /// providers on a failed network call would be worse than the bug this
    /// guards against.
    pub async fn fetch(api_key: &Secret) -> Option<Self> {
        Self::fetch_from(api_key, DEFAULT_OPENROUTER_BASE_URL).await
    }

    /// As [`fetch`](Self::fetch), against an explicit base URL (tests).
    pub async fn fetch_from(api_key: &Secret, base_url: &str) -> Option<Self> {
        let url = format!("{base_url}/v1/models");
        let client = reqwest::Client::builder()
            .user_agent("anseo/0.1 (+https://anseo.ai)")
            .build()
            .ok()?;
        let response = match client
            .get(&url)
            .bearer_auth(api_key.expose())
            .timeout(CATALOG_TIMEOUT)
            .send()
            .await
        {
            Ok(r) => r,
            Err(err) => {
                tracing::warn!(
                    event = "providers.openrouter_catalog_unavailable",
                    error = %crate::describe_reqwest_err(&err),
                    "could not fetch the OpenRouter model catalog; routed providers \
                     will be registered without slug validation"
                );
                return None;
            }
        };
        if !response.status().is_success() {
            tracing::warn!(
                event = "providers.openrouter_catalog_unavailable",
                status = response.status().as_u16(),
                "OpenRouter model catalog returned a non-success status; routed \
                 providers will be registered without slug validation"
            );
            return None;
        }
        let body: serde_json::Value = response.json().await.ok()?;
        Self::from_models_body(&body)
    }

    /// Parse a `/v1/models` body into a catalog. `None` when the response
    /// carries no usable ids, so a shape change degrades to "cannot validate"
    /// rather than to "every slug is invalid".
    pub fn from_models_body(body: &serde_json::Value) -> Option<Self> {
        let slugs: std::collections::HashSet<String> = body
            .get("data")?
            .as_array()?
            .iter()
            .filter_map(|m| m.get("id")?.as_str().map(str::to_string))
            .collect();
        if slugs.is_empty() {
            return None;
        }
        Some(Self { slugs })
    }

    pub fn accepts(&self, slug: &str) -> bool {
        self.slugs.contains(slug)
    }

    pub fn len(&self) -> usize {
        self.slugs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slugs.is_empty()
    }
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

    /// The routed default must come from OpenRouter's namespace, never from
    /// the vendor's own. Prefixing a native id is what made five of six
    /// providers fail every run with HTTP 400.
    #[test]
    fn default_routed_slug_uses_the_openrouter_namespace() {
        assert_eq!(
            default_routed_slug(&ProviderName::Anthropic).as_deref(),
            Some("anthropic/claude-sonnet-5.5")
        );
        assert_ne!(
            default_routed_slug(&ProviderName::Anthropic).as_deref(),
            Some("anthropic/claude-3-5-sonnet-20241022"),
            "the vendor-prefixed native id is not an OpenRouter slug"
        );
        // OpenAI is the one case where the two namespaces agree.
        assert_eq!(
            default_routed_slug(&ProviderName::Openai).as_deref(),
            Some("openai/gpt-4o-2024-08-06")
        );
        // Not routable through itself, nor as a plugin.
        assert_eq!(default_routed_slug(&ProviderName::Openrouter), None);
    }

    /// Every routed slug must carry a vendor segment, or OpenRouter rejects it
    /// on shape before it even looks the model up.
    #[test]
    fn every_routed_default_is_a_valid_slug_shape() {
        for target in [
            ProviderName::Openai,
            ProviderName::Anthropic,
            ProviderName::Gemini,
            ProviderName::Perplexity,
            ProviderName::Grok,
            ProviderName::Mistral,
        ] {
            let slug = default_routed_slug(&target).expect("routable provider has a slug");
            assert!(
                is_vendor_slash_model(&slug),
                "{target} routed default `{slug}` is not <vendor>/<model>"
            );
            let vendor = upstream_vendor_for(&target).expect("routable provider has a vendor");
            assert!(
                slug.starts_with(&format!("{vendor}/")),
                "{target} routed default `{slug}` does not sit under `{vendor}/`"
            );
        }
    }

    /// The orchestrator passes the NATIVE default when nothing is configured;
    /// the routed provider has to translate it rather than prefix it.
    #[test]
    fn unconfigured_model_is_translated_not_prefixed() {
        let routed =
            RoutedOpenRouterProvider::new(ProviderName::Anthropic, Secret::new("k")).unwrap();
        let native = ProviderName::Anthropic.default_model();
        assert_eq!(routed.upstream_model(native), "anthropic/claude-sonnet-5.5");
        // An explicit slug is passed through untouched.
        assert_eq!(
            routed.upstream_model("anthropic/claude-opus-5.5"),
            "anthropic/claude-opus-5.5"
        );
        // An explicitly configured bare name keeps the old prefixing, so the
        // operator's choice is not silently replaced by a default.
        assert_eq!(
            routed.upstream_model("some-custom-model"),
            "anthropic/some-custom-model"
        );
    }

    #[test]
    fn catalog_accepts_only_listed_slugs() {
        let body = serde_json::json!({
            "data": [
                {"id": "openai/gpt-4o-2024-08-06"},
                {"id": "anthropic/claude-3.5-sonnet"}
            ]
        });
        let catalog = OpenRouterCatalog::from_models_body(&body).expect("catalog");
        assert_eq!(catalog.len(), 2);
        assert!(catalog.accepts("openai/gpt-4o-2024-08-06"));
        // The vendor-prefixed native id — the shape that silently fails today.
        assert!(!catalog.accepts("anthropic/claude-3-5-sonnet-20241022"));
    }

    /// A shape we cannot read must mean "cannot validate", never "nothing is
    /// valid" — otherwise an API change would unregister every provider.
    #[test]
    fn catalog_is_none_for_unusable_bodies() {
        assert!(OpenRouterCatalog::from_models_body(&serde_json::json!({})).is_none());
        assert!(OpenRouterCatalog::from_models_body(&serde_json::json!({"data": []})).is_none());
        assert!(
            OpenRouterCatalog::from_models_body(&serde_json::json!({"data": "nope"})).is_none()
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
