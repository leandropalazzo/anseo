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
//! login command. When a provider declared in `anseo.yaml` has no secret
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

use anseo_core::{
    default_chain, get_provider_secret, Config, ProviderName, Secret, SecretStoreError,
};

use crate::{
    gemini::GeminiProvider,
    grok::GrokProvider,
    mistral::MistralProvider,
    openrouter::{
        default_routed_slug, OpenRouterCatalog, OpenRouterProvider, RoutedOpenRouterProvider,
    },
    perplexity::PerplexityProvider,
    AnthropicProvider, OpenAiProvider, Provider, ProviderRegistry,
};

/// Resolve the API secret for `provider`, scoped to `project_id` (Story 36.7).
///
/// Resolution order:
/// 1. The provider's env-var override (process-global; env vars are not
///    per-project). Useful in CI without touching the keychain.
/// 2. The project-scoped keychain key `"<project_id>:<provider>"`.
/// 3. The legacy global keychain key `"<provider>"` (back-compat for
///    deployments that stored keys before per-project keying existed).
///
/// Returns `Ok(None)` when no secret is configured — callers should skip
/// registering a client, letting the orchestrator synthesise a `failed` record
/// via `unregistered_record`. Any non-`NotFound` store error is surfaced as
/// `Err`.
pub fn resolve_provider_secret(
    provider: &ProviderName,
    project_id: &str,
) -> Result<Option<Secret>, SecretStoreError> {
    let env_var = env_var_for(provider);
    if let Ok(v) = std::env::var(env_var) {
        if !v.is_empty() {
            return Ok(Some(Secret::new(v)));
        }
    }
    let store = default_chain();
    // Project-scoped key first, then fall back to the legacy global key.
    match get_provider_secret(&store, project_id, &provider.as_wire_str()) {
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
/// 1. Providers declared in `anseo.yaml`. A declared provider with no
///    configured secret is skipped with a warning (orchestrator synthesises a
///    `failed` record via `unregistered_record`).
/// 2. Any *other* first-class provider that has a secret in the env/keychain
///    chain. This makes dashboard-saved keys usable even when the provider was
///    never listed in `anseo.yaml` — the previous behaviour silently ignored
///    such keys. Handlers source the model from `ProviderName::default_model`.
pub fn build_real_registry(config: &Config) -> Result<ProviderRegistry, SecretStoreError> {
    build_real_registry_with_catalog(config, None)
}

/// As [`build_real_registry`], but refusing to register an OpenRouter-routed
/// provider whose slug the aggregator does not list.
///
/// `catalog` is `None` for callers that cannot reach the network or do not
/// want to pay for the fetch; validation is then skipped and behaviour is
/// identical to [`build_real_registry`].
pub fn build_real_registry_with_catalog(
    config: &Config,
    catalog: Option<&OpenRouterCatalog>,
) -> Result<ProviderRegistry, SecretStoreError> {
    // Story 36.7: provider secrets are keyed per project. Derive the project
    // identity from the config (the same stable `ProjectId` used elsewhere) so
    // each project resolves its own keys, with a back-compat fall through to
    // legacy global keys inside `resolve_provider_secret`.
    let project_id = config.project_id().to_string();
    let mut registry: ProviderRegistry = HashMap::new();
    for provider_cfg in &config.providers {
        // Plugin providers are registered by the plugin loader, not from the
        // first-party secret chain. Skip them here.
        if provider_cfg.name.is_plugin() {
            continue;
        }
        let Some(secret) = resolve_provider_secret(&provider_cfg.name, &project_id)? else {
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
        if let Some(secret) = resolve_provider_secret(&name, &project_id)? {
            tracing::info!(
                provider = %name,
                "registering provider from stored key (not declared in config)"
            );
            registry.insert(name.clone(), build_client(&name, secret));
        }
    }

    if let Some(openrouter_secret) =
        resolve_provider_secret(&ProviderName::Openrouter, &project_id)?
    {
        for name in OPENROUTER_ROUTABLE_PROVIDERS {
            if registry.contains_key(&name) {
                continue;
            }
            // The slug this fallback would send is the provider's own default
            // model behind OpenRouter's vendor segment — and that is only a
            // real OpenRouter slug by coincidence. When the catalog says it is
            // not one, registering the client would buy nothing but a run that
            // fails with `provider_invalid_response` every time it fires.
            if let (Some(catalog), Some(slug)) = (catalog, default_routed_slug(&name)) {
                if !catalog.accepts(&slug) {
                    tracing::warn!(
                        event = "providers.openrouter_slug_unknown",
                        provider = %name,
                        slug = %slug,
                        "OpenRouter does not list `{slug}`, so `{name}` is not registered \
                         through it — every run would fail as provider_invalid_response. \
                         Declare an `openrouter` provider with an explicit `models:` list \
                         of slugs from https://openrouter.ai/api/v1/models instead"
                    );
                    continue;
                }
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

    // An empty registry cannot run anything, and the symptom is mute: the
    // worker sweeps forever reporting `dispatched_events = 0`. Secrets are
    // resolved under the project derived from `anseo.yaml`, so a key saved
    // against a different project leaves this empty while the dashboard still
    // shows it configured. Name the project the lookup actually used.
    if registry.is_empty() {
        tracing::warn!(
            event = "providers.registry_empty",
            project = %project_id,
            "no provider has a usable key for project `{project_id}`; scheduled runs will \
             dispatch nothing. Provider secrets are project-scoped — check the key was saved \
             for this project, and that brand.name in anseo.yaml names it"
        );
    }

    Ok(registry)
}
