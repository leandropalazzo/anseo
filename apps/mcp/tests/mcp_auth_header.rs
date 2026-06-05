//! GA criterion mcp-8 — X-Anseo-Project header forwarded on every /v1 call.
//!
//! ApiClient (apps/mcp/src/http_client.rs) pins the header on every
//! RequestBuilder it produces. These tests verify that without a live
//! server — `reqwest::RequestBuilder::build()` materialises the headers
//! into the Request struct for inspection.

use anseo_mcp::http_client::ApiClient;

#[test]
fn get_sends_project_header() {
    let client = ApiClient::new(
        "http://127.0.0.1:9999".to_string(),
        "test-key".to_string(),
        "my-project".to_string(),
    )
    .expect("ApiClient::new must not fail");
    let req = client.get("/v1/visibility").build().expect("build");
    let hdr = req
        .headers()
        .get("X-Anseo-Project")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        hdr,
        Some("my-project"),
        "X-Anseo-Project must be forwarded on GET"
    );
}

#[test]
fn post_sends_project_header() {
    let client = ApiClient::new(
        "http://127.0.0.1:9999".to_string(),
        "test-key".to_string(),
        "my-project".to_string(),
    )
    .expect("ApiClient::new must not fail");
    let req = client.post("/v1/prompt-runs").build().expect("build");
    let hdr = req
        .headers()
        .get("X-Anseo-Project")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        hdr,
        Some("my-project"),
        "X-Anseo-Project must be forwarded on POST"
    );
}
