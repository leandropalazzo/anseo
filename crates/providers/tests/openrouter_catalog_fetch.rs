//! The catalog fetch itself: a good response parses, and every failure mode
//! degrades to `None` ("cannot validate") rather than to an empty catalog
//! ("nothing is valid"), which would unregister every routed provider.

use anseo_core::Secret;
use anseo_providers::openrouter::OpenRouterCatalog;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn fetch_parses_the_model_ids() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {"id": "openai/gpt-4o-2024-08-06", "name": "GPT-4o"},
                {"id": "anthropic/claude-3.5-sonnet", "name": "Claude"}
            ]
        })))
        .mount(&server)
        .await;

    let catalog = OpenRouterCatalog::fetch_from(&Secret::new("k"), &server.uri())
        .await
        .expect("catalog");

    assert_eq!(catalog.len(), 2);
    assert!(catalog.accepts("anthropic/claude-3.5-sonnet"));
    assert!(!catalog.accepts("anthropic/claude-3-5-sonnet-20241022"));
}

#[tokio::test]
async fn non_success_yields_no_catalog() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    assert!(
        OpenRouterCatalog::fetch_from(&Secret::new("k"), &server.uri())
            .await
            .is_none(),
        "a 500 must not be read as an empty catalog"
    );
}

#[tokio::test]
async fn unreachable_host_yields_no_catalog() {
    // Bind then drop: the port is closed, so the connect is refused with no
    // network access and no waiting.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    assert!(
        OpenRouterCatalog::fetch_from(&Secret::new("k"), &format!("http://127.0.0.1:{port}"))
            .await
            .is_none(),
        "an unreachable aggregator must not strip providers"
    );
}
