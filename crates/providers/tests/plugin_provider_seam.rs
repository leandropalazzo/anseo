//! Story 17.6 — plugin Provider seam (FR-52, AD-Phase3-PluginsCannotRegisterMcpTools).
//!
//! A loaded plugin Provider, registered under `ProviderName::Plugin`, must run
//! through the same orchestrator path as a first-party Provider and produce a
//! `PromptRunRecord` whose shape is indistinguishable from a first-party row.
//! The `provider: "plugin:test.mock-provider"` wire string is the only field
//! that betrays its origin — every other column is populated identically.

use std::collections::HashMap;
use std::sync::Arc;

use anseo_core::{Config, ProviderName};
use anseo_providers::orchestrator::{Orchestrator, OrchestratorFilter, PromptRunStatus};
use anseo_providers::{MockProvider, PluginProvider, ProviderRegistry};

const FIRST_PARTY_YAML: &str = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: p1
    text: "first prompt"
providers:
  - name: openai
    model: mock-model
"#;

const PLUGIN_YAML: &str = r#"
schema_version: '0.1'
brand:
  name: Acme
prompts:
  - name: p1
    text: "first prompt"
providers:
  - name: "plugin:test.mock-provider"
    model: plugin-model
"#;

#[test]
fn plugin_provider_name_parses_from_yaml() {
    let cfg = Config::from_yaml_str(PLUGIN_YAML).unwrap();
    assert_eq!(
        cfg.providers[0].name,
        ProviderName::Plugin("test.mock-provider".into())
    );
    // Wire round-trip stays a plain string, not a tagged enum object.
    let json = serde_json::to_value(&cfg.providers[0].name).unwrap();
    assert_eq!(json, serde_json::json!("plugin:test.mock-provider"));
}

#[tokio::test]
async fn plugin_prompt_run_row_is_shape_indistinguishable_from_first_party() {
    // First-party run.
    let fp_cfg = Config::from_yaml_str(FIRST_PARTY_YAML).unwrap();
    let mut fp_registry: ProviderRegistry = HashMap::new();
    fp_registry.insert(
        ProviderName::Openai,
        Arc::new(
            MockProvider::new(ProviderName::Openai)
                .accept_model("mock-model")
                .queue_response("hello"),
        ),
    );
    let fp_records = Orchestrator::new(fp_cfg, fp_registry)
        .run_all(OrchestratorFilter::default())
        .await;
    assert_eq!(fp_records.len(), 1);
    let fp = &fp_records[0];

    // Plugin run.
    let pl_cfg = Config::from_yaml_str(PLUGIN_YAML).unwrap();
    let mut pl_registry: ProviderRegistry = HashMap::new();
    pl_registry.insert(
        ProviderName::Plugin("test.mock-provider".into()),
        Arc::new(
            PluginProvider::new("test.mock-provider")
                .accept_model("plugin-model")
                .queue_response("hello"),
        ),
    );
    let pl_records = Orchestrator::new(pl_cfg, pl_registry)
        .run_all(OrchestratorFilter::default())
        .await;
    assert_eq!(pl_records.len(), 1);
    let pl = &pl_records[0];

    // Both succeeded and carry the same populated-field profile.
    assert_eq!(fp.status, PromptRunStatus::Ok);
    assert_eq!(pl.status, PromptRunStatus::Ok);
    assert_eq!(fp.prompt_name, pl.prompt_name);
    assert_eq!(fp.message_text.is_some(), pl.message_text.is_some());
    assert_eq!(fp.error_kind.is_none(), pl.error_kind.is_none());
    assert_eq!(fp.error_message.is_none(), pl.error_message.is_none());
    assert!(pl.finished_at.is_some());
    assert!(!pl.provider_model_version.is_empty());
    assert!(pl.message_text.is_some());

    // The only distinguishing field is the provider wire string.
    assert_eq!(fp.provider.as_wire_str(), "openai");
    assert_eq!(pl.provider.as_wire_str(), "plugin:test.mock-provider");
    assert!(pl.provider.is_plugin());
    assert!(!fp.provider.is_plugin());
}

#[tokio::test]
async fn unregistered_plugin_provider_yields_failed_row_like_first_party() {
    // A declared-but-unregistered plugin provider gets the same synthesised
    // `failed` row treatment as a first-party provider with no key.
    let cfg = Config::from_yaml_str(PLUGIN_YAML).unwrap();
    let empty: ProviderRegistry = HashMap::new();
    let records = Orchestrator::new(cfg, empty)
        .run_all(OrchestratorFilter::default())
        .await;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, PromptRunStatus::Failed);
    assert_eq!(
        records[0].provider.as_wire_str(),
        "plugin:test.mock-provider"
    );
}
