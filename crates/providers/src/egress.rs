//! Provider endpoint allow-list checks.
//!
//! First-party providers must talk only to their canonical API hosts. Tests and
//! local development may use loopback wiremock endpoints; other private/link-local
//! literals remain forbidden by the shared core egress classifier.

use anseo_core::{validate_host_literal, ProviderName};

use crate::ProviderError;

pub fn validate_provider_base_url(
    provider: &ProviderName,
    base_url: &str,
) -> Result<(), ProviderError> {
    let parsed = reqwest::Url::parse(base_url).map_err(|e| {
        ProviderError::network(format!("provider base URL `{base_url}` is invalid: {e}"))
    })?;
    let scheme = parsed.scheme();
    let host = parsed.host_str().ok_or_else(|| {
        ProviderError::network(format!("provider base URL `{base_url}` has no host"))
    })?;

    if is_loopback_dev_host(host) {
        if scheme == "http" || scheme == "https" {
            return Ok(());
        }
        return Err(ProviderError::network(format!(
            "provider endpoint `{base_url}` must use http(s)"
        )));
    }

    validate_host_literal(host).map_err(|e| {
        ProviderError::network(format!(
            "provider endpoint `{base_url}` rejected by egress policy: {e}"
        ))
    })?;

    if scheme != "https" {
        return Err(ProviderError::network(format!(
            "provider endpoint `{base_url}` must use https"
        )));
    }

    let Some(allowed) = canonical_host(provider) else {
        return Ok(());
    };
    if host.eq_ignore_ascii_case(allowed) {
        Ok(())
    } else {
        Err(ProviderError::network(format!(
            "provider endpoint `{host}` is not allow-listed for `{provider}` (expected `{allowed}`)"
        )))
    }
}

fn canonical_host(provider: &ProviderName) -> Option<&'static str> {
    match provider {
        ProviderName::Openai => Some("api.openai.com"),
        ProviderName::Anthropic => Some("api.anthropic.com"),
        ProviderName::Gemini => Some("generativelanguage.googleapis.com"),
        ProviderName::Perplexity => Some("api.perplexity.ai"),
        ProviderName::Grok => Some("api.x.ai"),
        ProviderName::Mistral => Some("api.mistral.ai"),
        ProviderName::Openrouter => Some("openrouter.ai"),
        ProviderName::Plugin(_) => None,
    }
}

fn is_loopback_dev_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host == "[::1]"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_canonical_provider_hosts() {
        for (provider, base) in [
            (ProviderName::Openai, "https://api.openai.com"),
            (ProviderName::Anthropic, "https://api.anthropic.com"),
            (
                ProviderName::Gemini,
                "https://generativelanguage.googleapis.com",
            ),
            (ProviderName::Perplexity, "https://api.perplexity.ai"),
            (ProviderName::Grok, "https://api.x.ai"),
            (ProviderName::Mistral, "https://api.mistral.ai"),
            (ProviderName::Openrouter, "https://openrouter.ai/api"),
        ] {
            validate_provider_base_url(&provider, base).expect(base);
        }
    }

    #[test]
    fn rejects_unpinned_provider_host() {
        let err =
            validate_provider_base_url(&ProviderName::Openai, "https://evil.example").unwrap_err();
        assert!(err.message.contains("not allow-listed"));
    }

    #[test]
    fn rejects_forbidden_ip_literals_even_when_https() {
        let err = validate_provider_base_url(&ProviderName::Openai, "https://169.254.169.254")
            .unwrap_err();
        assert!(err.message.contains("egress policy"));
    }

    #[test]
    fn accepts_loopback_for_wiremock_fixtures() {
        validate_provider_base_url(&ProviderName::Openai, "http://127.0.0.1:1234").unwrap();
        validate_provider_base_url(&ProviderName::Openai, "http://localhost:1234").unwrap();
    }
}
