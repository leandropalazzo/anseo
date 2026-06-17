//! Separate HTTP client for `search_benchmarks` (FR-51) — the **privacy floor**.
//!
//! Per `architecture-phase3-mcp-server.md` §3.6 / §4, the benchmark tool hits
//! the **public benchmark service**, NOT the local `/v1` API, and it must
//! **never transmit any local-deployment data**. This client is deliberately
//! distinct from [`crate::http_client::ApiClient`]:
//!
//! - It carries **no API key** (no `Authorization` header).
//! - It carries **no project/brand identifier** (no `X-Anseo-Project`,
//!   no `Project-Id`).
//! - Its `User-Agent` is a fixed `anseo-mcp/<version> benchmark-search`
//!   string that contains no brand name.
//!
//! The structural guarantee is that this client never *receives* the api key
//! or project — `Tool::call` builds it from environment only, so a brand/
//! project value is unreachable from this code path. `tests/benchmark_privacy.rs`
//! snapshots the outbound request and asserts the absence of those identifiers
//! (GA criterion `[mcp-6]`).

use std::time::Duration;

use reqwest::header::USER_AGENT;

/// Default public benchmark service base URL (overridable via
/// `ANSEO_BENCHMARK_URL` for self-hosted / test deployments).
const DEFAULT_BENCHMARK_URL: &str = "https://benchmark.anseo.ai";

/// Fixed User-Agent for benchmark requests. No brand name (privacy floor).
pub fn benchmark_user_agent() -> String {
    format!("anseo-mcp/{} benchmark-search", env!("CARGO_PKG_VERSION"))
}

/// Headerless client to the public benchmark service.
#[derive(Clone, Debug)]
pub struct BenchmarkClient {
    inner: reqwest::Client,
    base_url: String,
}

impl BenchmarkClient {
    /// Build from environment. Reads `ANSEO_BENCHMARK_URL` (default
    /// [`DEFAULT_BENCHMARK_URL`]). Never reads any API key or project value —
    /// that is the point.
    pub fn from_env() -> anyhow::Result<Self> {
        let base_url = std::env::var("ANSEO_BENCHMARK_URL")
            .unwrap_or_else(|_| DEFAULT_BENCHMARK_URL.to_string());
        Self::with_base_url(base_url)
    }

    /// Build against an explicit base URL (used by tests).
    pub fn with_base_url(base_url: String) -> anyhow::Result<Self> {
        let inner = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self { inner, base_url })
    }

    /// Build a `GET` request for the public benchmark aggregates endpoint.
    ///
    /// Attaches **only** the allow-listed `query` params and the fixed
    /// benchmark `User-Agent`. No auth, no project header — ever.
    pub fn aggregates_request(&self, query: &[(String, String)]) -> reqwest::RequestBuilder {
        let url = format!("{}/v1/benchmark/aggregates/search", self.base_url);
        self.inner
            .get(url)
            .header(USER_AGENT, benchmark_user_agent())
            .query(query)
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
