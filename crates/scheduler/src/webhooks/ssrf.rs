//! Webhook SSRF guard for declaration and delivery.
//!
//! Declaration rejects obvious dangerous literals. Delivery resolves the target
//! hostname, rejects forbidden answers, and builds a reqwest client pinned to
//! those vetted socket addresses so DNS rebinding cannot swap the connection
//! target after validation.

use std::net::SocketAddr;
use std::time::Duration;

use anseo_core::{validate_host_literal, validate_resolved_ip};
use reqwest::{Client, Url};
use tokio::net::lookup_host;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebhookSsrfError {
    #[error("webhook URL `{0}` is invalid: {1}")]
    InvalidUrl(String, String),
    #[error("webhook URL `{0}` must use https, or http for localhost/loopback fixtures")]
    InvalidScheme(String),
    #[error("webhook URL `{0}` has no host")]
    MissingHost(String),
    #[error("webhook target `{0}` rejected by egress policy: {1}")]
    ForbiddenTarget(String, String),
    #[error("webhook target `{0}` did not resolve to any address")]
    EmptyResolution(String),
    #[error("webhook target `{0}` DNS resolution failed: {1}")]
    ResolveFailed(String, String),
    #[error("failed to build pinned webhook HTTP client: {0}")]
    ClientBuild(String),
}

pub fn validate_webhook_declaration_url(raw: &str) -> Result<Url, WebhookSsrfError> {
    let parsed = parse_url(raw)?;
    validate_url_shape(&parsed)?;
    let host = host_for_policy(&parsed)?;
    if parsed.scheme() == "http" && is_loopback_dev_host(&host) {
        return Ok(parsed);
    }
    validate_host_literal(&host)
        .map_err(|e| WebhookSsrfError::ForbiddenTarget(raw.to_string(), e.to_string()))?;
    Ok(parsed)
}

pub async fn pinned_client_for_delivery(
    raw: &str,
    timeout: Duration,
) -> Result<(Client, Url), WebhookSsrfError> {
    let parsed = validate_webhook_declaration_url(raw)?;
    let host = host_for_policy(&parsed)?;
    let port = parsed.port_or_known_default().ok_or_else(|| {
        WebhookSsrfError::InvalidUrl(raw.to_string(), "unknown port for scheme".into())
    })?;

    let resolved: Vec<SocketAddr> = lookup_host((host.as_str(), port))
        .await
        .map_err(|e| WebhookSsrfError::ResolveFailed(raw.to_string(), e.to_string()))?
        .collect();
    if resolved.is_empty() {
        return Err(WebhookSsrfError::EmptyResolution(raw.to_string()));
    }

    if !is_loopback_dev_host(&host) {
        for addr in &resolved {
            validate_resolved_ip(&host, addr.ip())
                .map_err(|e| WebhookSsrfError::ForbiddenTarget(raw.to_string(), e.to_string()))?;
        }
    }

    let client = Client::builder()
        .timeout(timeout)
        .resolve_to_addrs(&host, &resolved)
        .pool_idle_timeout(Some(Duration::from_secs(90)))
        .user_agent("anseo-webhook-dispatcher/0.1")
        .build()
        .map_err(|e| WebhookSsrfError::ClientBuild(e.to_string()))?;
    Ok((client, parsed))
}

fn parse_url(raw: &str) -> Result<Url, WebhookSsrfError> {
    Url::parse(raw).map_err(|e| WebhookSsrfError::InvalidUrl(raw.to_string(), e.to_string()))
}

fn validate_url_shape(url: &Url) -> Result<(), WebhookSsrfError> {
    let scheme = url.scheme();
    let host = host_for_policy(url)?;
    if scheme == "https" {
        return Ok(());
    }
    if scheme == "http" && is_loopback_dev_host(&host) {
        return Ok(());
    }
    Err(WebhookSsrfError::InvalidScheme(url.to_string()))
}

fn host_for_policy(url: &Url) -> Result<String, WebhookSsrfError> {
    url.host_str()
        .map(|h| h.trim_matches('[').trim_matches(']').to_string())
        .ok_or_else(|| WebhookSsrfError::MissingHost(url.to_string()))
}

fn is_loopback_dev_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_accepts_https_public_name() {
        validate_webhook_declaration_url("https://hooks.example.com/anseo").unwrap();
    }

    #[test]
    fn declaration_accepts_loopback_http_for_fixtures() {
        validate_webhook_declaration_url("http://127.0.0.1:8080/hook").unwrap();
        validate_webhook_declaration_url("http://localhost:8080/hook").unwrap();
    }

    #[test]
    fn declaration_rejects_plaintext_external() {
        let err = validate_webhook_declaration_url("http://example.com/hook").unwrap_err();
        assert!(err.to_string().contains("must use https"));
    }

    #[test]
    fn declaration_rejects_metadata_and_private_literals() {
        for target in [
            "https://169.254.169.254/latest/meta-data",
            "https://0251.0376.0251.0376/latest/meta-data",
            "https://2852039166/latest/meta-data",
            "https://10.0.0.1/hook",
            "https://[::1]/hook",
        ] {
            let err = validate_webhook_declaration_url(target).unwrap_err();
            assert!(err.to_string().contains("egress policy"), "{target}: {err}");
        }
    }
}
