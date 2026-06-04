//! Loopback HTTP client to the local `/v1` REST surface.
//!
//! Per AD-Phase3-MCP-Process-Model (architecture-phase3-mcp-server.md §5) the
//! MCP server never bypasses the API to the storage layer. Every call hits
//! `/v1` over loopback HTTP and forwards both `X-OpenGEO-API-Key: <key>`
//! (the header the API's `require_api_key` middleware reads) and
//! `X-OpenGEO-Project: <project>` (L2 of the Phase 3 kickoff decisions).
//!
//! Story 16.1 ships the client wrapper. The tool stubs do NOT call it; the
//! handlers in 16.2-16.5 do. We construct it at boot so config errors surface
//! immediately (and so it's available to the tool registry).

use std::time::Duration;

/// Thin wrapper around a `reqwest::Client` that pins the auth + project
/// headers on every outbound call.
#[derive(Clone, Debug)]
pub struct ApiClient {
    inner: reqwest::Client,
    base_url: String,
    api_key: String,
    project: String,
}

impl ApiClient {
    /// Build a client. Returns an error only if the underlying TLS / DNS
    /// resolver fails to initialize — never on bad URL (validated lazily per
    /// request).
    pub fn new(base_url: String, api_key: String, project: String) -> anyhow::Result<Self> {
        let inner = reqwest::Client::builder()
            // Per architecture §7.1 — per-tool 30s budget; the per-call
            // timeout here is the inner HTTP timeout, well below.
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            inner,
            base_url,
            api_key,
            project,
        })
    }

    /// Issue a `GET` against `<base_url><path>`. Always carries the auth +
    /// project headers per L2.
    #[allow(dead_code)] // exercised by stories 16.2-16.5
    pub fn get(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        self.inner
            .get(url)
            .header("X-OpenGEO-API-Key", &self.api_key)
            .header("X-OpenGEO-Project", &self.project)
    }

    /// Issue a `POST` against `<base_url><path>`. Always carries the auth +
    /// project headers per L2.
    #[allow(dead_code)] // exercised by stories 16.2-16.5
    pub fn post(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        self.inner
            .post(url)
            .header("X-OpenGEO-API-Key", &self.api_key)
            .header("X-OpenGEO-Project", &self.project)
    }

    /// Issue a `PATCH` against `<base_url><path>`. Always carries the auth +
    /// project headers per L2.
    #[allow(dead_code)] // exercised by story 19.7 recommend.* tools
    pub fn patch(&self, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        self.inner
            .patch(url)
            .header("X-OpenGEO-API-Key", &self.api_key)
            .header("X-OpenGEO-Project", &self.project)
    }

    #[allow(dead_code)]
    pub fn project(&self) -> &str {
        &self.project
    }
}
