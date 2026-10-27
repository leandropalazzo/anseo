//! Registration must not hand out a routed provider that OpenRouter cannot
//! serve.
//!
//! The fallback sends `<vendor>/<provider default model>`, which is a real
//! OpenRouter slug only by coincidence — true for OpenAI, false for most
//! others. Registering one regardless produces a client whose every run comes
//! back `provider_invalid_response`, so the catalog gates it.

use anseo_core::{Config, ProviderName};
use anseo_providers::openrouter::OpenRouterCatalog;
use anseo_providers::registry::build_real_registry_with_catalog;

/// Config declaring no providers: the routed fallbacks are the only entries
/// registration can add, which is exactly what this test is about.
fn bare_config() -> Config {
    Config::from_yaml_str(
        r#"
schema_version: '0.1'
brand:
  name: "catalog-gate-fixture"
prompts:
  - name: p
    text: "t"
providers: []
"#,
    )
    .expect("fixture config parses")
}

#[test]
fn routed_providers_are_gated_on_the_catalog() {
    // `resolve_provider_secret` consults the env before the keychain, so this
    // stands in for a stored OpenRouter key without touching the real store.
    std::env::set_var("OPENROUTER_API_KEY", "ogeo-test-key");

    let config = bare_config();

    // A catalog that lists OpenAI's slug but not Anthropic's prefixed native id.
    let catalog = OpenRouterCatalog::from_models_body(&serde_json::json!({
        "data": [{"id": "openai/gpt-4o-2024-08-06"}]
    }))
    .expect("catalog");

    let registry =
        build_real_registry_with_catalog(&config, Some(&catalog)).expect("registry builds");

    assert!(
        registry.contains_key(&ProviderName::Openai),
        "openai/gpt-4o-2024-08-06 is listed, so OpenAI must register"
    );
    assert!(
        !registry.contains_key(&ProviderName::Anthropic),
        "anthropic/claude-3-5-sonnet-20241022 is not listed, so Anthropic must be refused"
    );

    // Without a catalog nothing is validated, and the old behaviour stands —
    // a network failure must not strip providers.
    let unvalidated = build_real_registry_with_catalog(&config, None).expect("registry builds");
    assert!(
        unvalidated.contains_key(&ProviderName::Anthropic),
        "with no catalog the fallback registers as before"
    );

    std::env::remove_var("OPENROUTER_API_KEY");
}
