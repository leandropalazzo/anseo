//! Phase 2 Epic 11 — wiremock integration tests for every Phase 2 Provider.
//!
//! Each test stands up a `wiremock` server, points the adapter at it via
//! `with_base_url`, and exercises one happy path + one error class. The
//! tests don't need a live DB — they validate the HTTP wire shape and
//! the closed-error-taxonomy mapping.

use std::time::Duration;

use anseo_core::{ProviderErrorKind, ProviderName, RequestId, Secret};
use anseo_providers::{
    anthropic::AnthropicProvider, gemini::GeminiProvider, grok::GrokProvider,
    mistral::MistralProvider, openai::OpenAiProvider, openrouter::OpenRouterProvider,
    perplexity::PerplexityProvider, Provider, ProviderRequest,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture_request(prompt: &str, model: &str) -> ProviderRequest {
    ProviderRequest {
        request_id: RequestId::new(),
        prompt_text: prompt.to_string(),
        model: model.to_string(),
        request_parameters: serde_json::json!({}),
        timeout: Duration::from_secs(5),
    }
}

// ---------------------------------------------------------------------
// OpenAI (Story 2.4, frozen)
// ---------------------------------------------------------------------

#[tokio::test]
async fn openai_happy_path_round_trips_message_text() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "gpt-4o-2024-08-06",
            "choices": [
                {"message": {"role": "assistant", "content": "Pinecone is leading."}}
            ]
        })))
        .mount(&mock)
        .await;

    let provider = OpenAiProvider::with_base_url(Secret::new("sk-test"), mock.uri());
    let response = provider
        .run(fixture_request("best vector db", "gpt-4o-2024-08-06"))
        .await
        .expect("happy path");
    assert_eq!(response.provider, ProviderName::Openai);
    assert_eq!(response.model, "gpt-4o-2024-08-06");
    assert_eq!(response.message_text, "Pinecone is leading.");
}

#[tokio::test]
async fn openai_401_maps_to_provider_unauthorized() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("bad key"))
        .mount(&mock)
        .await;
    let provider = OpenAiProvider::with_base_url(Secret::new("sk-test"), mock.uri());
    let err = provider
        .run(fixture_request("p", "gpt-4o-2024-08-06"))
        .await
        .expect_err("must error");
    assert_eq!(err.kind, ProviderErrorKind::ProviderUnauthorized);
}

// ---------------------------------------------------------------------
// Anthropic (Story 2.4, frozen)
// ---------------------------------------------------------------------

#[tokio::test]
async fn anthropic_happy_path_concatenates_text_blocks() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "content": [
                {"type": "text", "text": "Alpha"},
                {"type": "text", "text": "Beta"}
            ]
        })))
        .mount(&mock)
        .await;
    let provider = AnthropicProvider::with_base_url(Secret::new("sk-ant"), mock.uri());
    let response = provider
        .run(fixture_request("p", "claude-3-5-sonnet-20241022"))
        .await
        .expect("happy");
    assert_eq!(response.provider, ProviderName::Anthropic);
    assert_eq!(response.message_text, "Alpha\nBeta");
}

#[tokio::test]
async fn anthropic_429_maps_to_rate_limited() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock)
        .await;
    let provider = AnthropicProvider::with_base_url(Secret::new("sk-ant"), mock.uri());
    let err = provider
        .run(fixture_request("p", "claude-3-5-sonnet-20241022"))
        .await
        .expect_err("must error");
    assert_eq!(err.kind, ProviderErrorKind::ProviderRateLimited);
}

// ---------------------------------------------------------------------
// Gemini (Story 11.1)
// ---------------------------------------------------------------------

#[tokio::test]
async fn gemini_happy_path_parses_candidates_content_parts() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-pro-002:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Pinecone leads in serverless vector search."}]
                }
            }]
        })))
        .mount(&mock)
        .await;
    let provider = GeminiProvider::with_base_url(Secret::new("api-key"), mock.uri());
    let response = provider
        .run(fixture_request("p", "gemini-1.5-pro-002"))
        .await
        .expect("happy");
    assert_eq!(response.provider, ProviderName::Gemini);
    assert!(response.message_text.contains("Pinecone"));
}

#[tokio::test]
async fn gemini_400_with_api_key_message_maps_to_unauthorized() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/gemini-1.5-pro-002:generateContent"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string("API key not valid. Please pass a valid API key."),
        )
        .mount(&mock)
        .await;
    let provider = GeminiProvider::with_base_url(Secret::new("bad"), mock.uri());
    let err = provider
        .run(fixture_request("p", "gemini-1.5-pro-002"))
        .await
        .expect_err("error");
    assert_eq!(err.kind, ProviderErrorKind::ProviderUnauthorized);
}

// ---------------------------------------------------------------------
// Perplexity (Story 11.1)
// ---------------------------------------------------------------------

#[tokio::test]
async fn perplexity_happy_path_parses_openai_compat_shape() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "sonar-large-online-128k",
            "choices": [
                {"message": {"role": "assistant", "content": "Real-time response."}}
            ]
        })))
        .mount(&mock)
        .await;
    let provider = PerplexityProvider::with_base_url(Secret::new("pplx-tok"), mock.uri());
    let response = provider
        .run(fixture_request("news", "sonar-large-online-128k"))
        .await
        .expect("happy");
    assert_eq!(response.provider, ProviderName::Perplexity);
    assert_eq!(response.message_text, "Real-time response.");
}

#[tokio::test]
async fn perplexity_503_maps_to_5xx() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&mock)
        .await;
    let provider = PerplexityProvider::with_base_url(Secret::new("pplx-tok"), mock.uri());
    let err = provider
        .run(fixture_request("p", "sonar-large-online-128k"))
        .await
        .expect_err("error");
    assert_eq!(err.kind, ProviderErrorKind::Provider5xx);
}

// ---------------------------------------------------------------------
// Grok (Story 11.2)
// ---------------------------------------------------------------------

#[tokio::test]
async fn grok_happy_path_parses_openai_compat_shape() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "grok-2-1212",
            "choices": [{"message": {"role": "assistant", "content": "Grok answer."}}]
        })))
        .mount(&mock)
        .await;
    let provider = GrokProvider::with_base_url(Secret::new("xai-tok"), mock.uri());
    let response = provider
        .run(fixture_request("p", "grok-2-1212"))
        .await
        .expect("happy");
    assert_eq!(response.provider, ProviderName::Grok);
    assert!(response.message_text.contains("Grok"));
}

// ---------------------------------------------------------------------
// Mistral (Story 11.2)
// ---------------------------------------------------------------------

#[tokio::test]
async fn mistral_happy_path_parses_openai_compat_shape() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "mistral-large-latest",
            "choices": [{"message": {"role": "assistant", "content": "Mistral answer."}}]
        })))
        .mount(&mock)
        .await;
    let provider = MistralProvider::with_base_url(Secret::new("mi-tok"), mock.uri());
    let response = provider
        .run(fixture_request("p", "mistral-large-latest"))
        .await
        .expect("happy");
    assert_eq!(response.provider, ProviderName::Mistral);
}

// ---------------------------------------------------------------------
// OpenRouter (Story 11.3)
// ---------------------------------------------------------------------

#[tokio::test]
async fn openrouter_threads_upstream_model_into_raw_response_metadata() {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "openai/gpt-4o-2024-08-06",
            "choices": [{"message": {"role": "assistant", "content": "Aggregated."}}]
        })))
        .mount(&mock)
        .await;
    let provider = OpenRouterProvider::with_base_url(Secret::new("or-tok"), mock.uri());
    let response = provider
        .run(fixture_request("p", "openai/gpt-4o-2024-08-06"))
        .await
        .expect("happy");
    assert_eq!(response.provider, ProviderName::Openrouter);
    let upstream = response.raw_response["metadata"]["upstream_model"]
        .as_str()
        .expect("upstream_model populated");
    assert_eq!(upstream, "openai/gpt-4o-2024-08-06");
}

#[tokio::test]
async fn openrouter_validates_vendor_slash_model_shape() {
    let provider = OpenRouterProvider::new(Secret::new("or"));
    let err = provider
        .validate_model("gpt-4o-no-slash")
        .expect_err("must reject");
    assert_eq!(err.kind, ProviderErrorKind::ProviderUnsupportedModel);
}
