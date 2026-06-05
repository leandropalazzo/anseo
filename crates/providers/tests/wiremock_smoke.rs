//! Integration tests for the OpenAI / Anthropic adapters against `wiremock`.
//!
//! No real network. The adapters point at a wiremock server and we assert:
//! - 200 with the canonical response shape parses into a `ProviderResponse`
//! - 401 / 429 / 503 / non-JSON / timeout map to the closed taxonomy
//! - `x-api-key` / Bearer headers and `X-Anseo-Request-Id` propagate

use std::time::Duration;

use anseo_core::{ProviderErrorKind, Secret};
use anseo_providers::{AnthropicProvider, OpenAiProvider, Provider, ProviderRequest};
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn req_for(model: &str) -> ProviderRequest {
    ProviderRequest::new("What are the best vector databases?", model)
        .with_timeout(Duration::from_secs(5))
}

#[tokio::test]
async fn openai_success_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer sk-fixture"))
        .and(header_exists("X-Anseo-Request-Id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-test",
            "model": "gpt-4o-2024-08-06",
            "choices": [{
                "message": {"role": "assistant", "content": "Pinecone, Weaviate, Qdrant."}
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let p = OpenAiProvider::with_base_url(Secret::new("sk-fixture"), server.uri());
    let resp = p.run(req_for("gpt-4o-2024-08-06")).await.unwrap();
    assert_eq!(resp.model, "gpt-4o-2024-08-06");
    assert!(resp.message_text.contains("Pinecone"));
    assert_eq!(resp.raw_response["id"], "chatcmpl-test");
}

#[tokio::test]
async fn openai_401_maps_to_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("invalid_api_key"))
        .mount(&server)
        .await;
    let p = OpenAiProvider::with_base_url(Secret::new("bad"), server.uri());
    let err = p.run(req_for("gpt-4o-2024-08-06")).await.unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::ProviderUnauthorized);
}

#[tokio::test]
async fn openai_429_maps_to_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
        .mount(&server)
        .await;
    let p = OpenAiProvider::with_base_url(Secret::new("sk"), server.uri());
    let err = p.run(req_for("gpt-4o-2024-08-06")).await.unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::ProviderRateLimited);
}

#[tokio::test]
async fn openai_503_maps_to_5xx() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(503).set_body_string("down"))
        .mount(&server)
        .await;
    let p = OpenAiProvider::with_base_url(Secret::new("sk"), server.uri());
    let err = p.run(req_for("gpt-4o-2024-08-06")).await.unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::Provider5xx);
}

#[tokio::test]
async fn openai_non_json_maps_to_invalid_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>oops</html>"))
        .mount(&server)
        .await;
    let p = OpenAiProvider::with_base_url(Secret::new("sk"), server.uri());
    let err = p.run(req_for("gpt-4o-2024-08-06")).await.unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::ProviderInvalidResponse);
}

#[tokio::test]
async fn openai_timeout_maps_to_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(10))
                .set_body_string("won't reach"),
        )
        .mount(&server)
        .await;
    let p = OpenAiProvider::with_base_url(Secret::new("sk"), server.uri());
    let req =
        ProviderRequest::new("x", "gpt-4o-2024-08-06").with_timeout(Duration::from_millis(50));
    let err = p.run(req).await.unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::ProviderTimeout);
}

#[tokio::test]
async fn anthropic_success_round_trip() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-ant-fixture"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_test",
            "model": "claude-3-5-sonnet-20241022",
            "content": [
                {"type": "text", "text": "Vector DBs include Qdrant and Weaviate."}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let p = AnthropicProvider::with_base_url(Secret::new("sk-ant-fixture"), server.uri());
    let resp = p.run(req_for("claude-3-5-sonnet-20241022")).await.unwrap();
    assert!(resp.message_text.contains("Qdrant"));
    assert_eq!(resp.model, "claude-3-5-sonnet-20241022");
}

#[tokio::test]
async fn anthropic_401_maps_to_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;
    let p = AnthropicProvider::with_base_url(Secret::new("bad"), server.uri());
    let err = p
        .run(req_for("claude-3-5-sonnet-20241022"))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ProviderErrorKind::ProviderUnauthorized);
}
