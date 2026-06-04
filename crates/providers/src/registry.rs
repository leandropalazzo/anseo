//! Shared `ProviderRegistry` construction for CLI + API.
//!
//! Both `apps/cli/src/commands/run.rs` and `apps/api/src/routes/prompt_runs.rs`
//! need to materialize a [`ProviderRegistry`] from a [`Config`] using the
//! per-provider secret resolved from the chained secret store (env override
//! first, then keychain). This module is the single source of truth for
//! that wiring so the API path and the CLI path can never drift.
//!
//! ## Missing-key semantics
//!
//! Per FR-7/FR-8 the missing-key error names the relevant env var and the
//! login command. When a provider declared in `opengeo.yaml` has no secret
//! configured, `build_real_registry` simply **skips** registering a client
//! for it. The orchestrator's `unregistered_record` path then synthesises a
//! `failed` `PromptRunRecord` with `ProviderUnauthorized` and the
//! `"run \`ogeo login <provider>\`"` hint — see
//! `crates/providers/src/orchestrator.rs::unregistered_record`.
//!
//! This is the behaviour the API surface wants: a `POST /v1/prompt-runs`
//! against a provider with no key returns a persisted `failed` row rather
//! than 503-ing the whole request.

use std::collections::HashMap;
use std::sync::Arc;

use opengeo_core::{default_chain, Config, ProviderName, Secret, SecretStore, SecretStoreError};

use crate::{
    gemini::GeminiProvider,
    grok::GrokProvider,
    mistral::MistralProvider,
    openrouter::{OpenRouterProvider, RoutedOpenRouterProvider},
    perplexity::PerplexityProvider,
    AnthropicProvider, OpenAiProvider, Provider, ProviderRegistry,
};

/// Resolve the API secret for `provider` from env override or the default
/// keychain chain. Returns `Ok(None)` when no secret is configured — callers
/// should skip registering a client, letting the orchestrator synthesise a
/// `failed` record via `unregistered_record`.
///
/// Any non-`NotFound` store error is surfaced as `Err`.
pub fn resolve_provider_secret(
    provider: &ProviderName,
) -> Result<Option<Secret>, SecretStoreError> {
    let env_var = env_var_for(provider);
    if let Ok(v) = std::env::var(env_var) {
        if !v.is_empty() {
            return Ok(Some(Secret::new(v)));
        }
    }
    let store = default_chain();
    match store.get(&provider.as_wire_str()) {
        Ok(s) => Ok(Some(s)),
        Err(SecretStoreError::NotFound { .. }) => Ok(None),
        Err(other) => Err(other),
    }
}

fn env_var_for(provider: &ProviderName) -> &'static str {
    match provider {
        ProviderName::Openai => "OPENAI_API_KEY",
        ProviderName::Anthropic => "ANTHROPIC_API_KEY",
        ProviderName::Gemini => "GEMINI_API_KEY",
        ProviderName::Perplexity => "PERPLEXITY_API_KEY",
        ProviderName::Grok => "GROK_API_KEY",
        ProviderName::Mistral => "MISTRAL_API_KEY",
        ProviderName::Openrouter => "OPENROUTER_API_KEY",
        // Plugin providers do not source secrets from the first-party env/
        // keychain chain; their credentials (if any) are the plugin's concern.
        ProviderName::Plugin(_) => "",
    }
}

/// The first-class providers that source secrets from the env/keychain chain.
/// Used for the "any stored key" registry pass below. Excludes `Plugin`.
const FIRST_CLASS_PROVIDERS: [ProviderName; 7] = [
    ProviderName::Openai,
    ProviderName::Anthropic,
    ProviderName::Gemini,
    ProviderName::Perplexity,
    ProviderName::Grok,
    ProviderName::Mistral,
    ProviderName::Openrouter,
];

const OPENROUTER_ROUTABLE_PROVIDERS: [ProviderName; 6] = [
    ProviderName::Openai,
    ProviderName::Anthropic,
    ProviderName::Gemini,
    ProviderName::Perplexity,
    ProviderName::Grok,
    ProviderName::Mistral,
];

/// Construct a live client for a first-class provider from its resolved secret.
fn build_client(name: &ProviderName, secret: Secret) -> Arc<dyn Provider> {
    match name {
        ProviderName::Openai => Arc::new(OpenAiProvider::new(secret)),
        ProviderName::Anthropic => Arc::new(AnthropicProvider::new(secret)),
        ProviderName::Gemini => Arc::new(GeminiProvider::new(secret)),
        ProviderName::Perplexity => Arc::new(PerplexityProvider::new(secret)),
        ProviderName::Grok => Arc::new(GrokProvider::new(secret)),
        ProviderName::Mistral => Arc::new(MistralProvider::new(secret)),
        ProviderName::Openrouter => Arc::new(OpenRouterProvider::new(secret)),
        ProviderName::Plugin(_) => unreachable!("plugin providers are not first-class"),
    }
}

/// Build a [`ProviderRegistry`] from `config`.
///
/// Two passes:
/// 1. Providers declared in `opengeo.yaml`. A declared provider with no
///    configured secret is skipped with a warning (orchestrator synthesises a
///    `failed` record via `unregistered_record`).
/// 2. Any *other* first-class provider that has a secret in the env/keychain
///    chain. This makes dashboard-saved keys usable even when the provider was
///    never listed in `opengeo.yaml` — the previous behaviour silently ignored
///    such keys. Handlers source the model from `ProviderName::default_model`.
pub fn build_real_registry(config: &Config) -> Result<ProviderRegistry, SecretStoreError> {
    let mut registry: ProviderRegistry = HashMap::new();
    for provider_cfg in &config.providers {
        // Plugin providers are registered by the plugin loader, not from the
        // first-party secret chain. Skip them here.
        if provider_cfg.name.is_plugin() {
            continue;
        }
        let Some(secret) = resolve_provider_secret(&provider_cfg.name)? else {
            tracing::warn!(
                provider = %provider_cfg.name,
                env_var = env_var_for(&provider_cfg.name),
                "no API key configured; provider will report `no key configured` for every run"
            );
            continue;
        };
        registry.insert(
            provider_cfg.name.clone(),
            build_client(&provider_cfg.name, secret),
        );
    }

    // Second pass: register undeclared first-class providers that have a stored
    // key (e.g. saved via the dashboard) so they are immediately usable.
    for name in FIRST_CLASS_PROVIDERS {
        if registry.contains_key(&name) {
            continue;
        }
        if let Some(secret) = resolve_provider_secret(&name)? {
            tracing::info!(
                provider = %name,
                "registering provider from stored key (not declared in config)"
            );
            registry.insert(name.clone(), build_client(&name, secret));
        }
    }

    if let Some(openrouter_secret) = resolve_provider_secret(&ProviderName::Openrouter)? {
        for name in OPENROUTER_ROUTABLE_PROVIDERS {
            if registry.contains_key(&name) {
                continue;
            }
            if let Some(client) =
                RoutedOpenRouterProvider::new(name.clone(), openrouter_secret.clone())
            {
                tracing::info!(
                    provider = %name,
                    "registering OpenRouter fallback for provider"
                );
                registry.insert(name, Arc::new(client));
            }
        }
    }

    Ok(registry)
}
