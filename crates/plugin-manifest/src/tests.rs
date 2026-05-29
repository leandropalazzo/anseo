//! Unit tests for the plugin-manifest substrate crate (Story 17.1).
//!
//! Covered:
//!   * YAML round-trip of a fully-populated manifest.
//!   * Strict-parse: unknown capability tag is a hard error.
//!   * `validate()` catches empty name, invalid version, missing entry_point,
//!     parent-traversal entry_point.
//!   * `NewInstallRecord::from_manifest` carries the substrate placeholder
//!     trust-root literal so it's visible in any audit grep.

use super::*;
use std::path::PathBuf;

fn good_manifest() -> PluginManifest {
    PluginManifest {
        name: "priya.perplexity-pro-extractor".into(),
        version: "0.3.1".into(),
        description: "Higher-recall citation extraction.".into(),
        author: "Priya".into(),
        homepage: "https://example.com".into(),
        capabilities: vec![
            Capability::Network {
                allowlist: vec!["api.example.com".into()],
            },
            Capability::ReadSecret {
                keys: vec!["plugin:priya.perplexity-pro".into()],
            },
            Capability::EmitEvent {
                kinds: vec!["citation.extracted".into()],
            },
            Capability::ExtractorConfidenceOverride,
            Capability::AnalyticsWindow {
                windows: vec!["30d".into()],
            },
        ],
        plugin_type: PluginType::Extractor,
        entry_point: PathBuf::from("bin/extractor.wasm"),
    }
}

#[test]
fn manifest_yaml_roundtrip() {
    let original = good_manifest();
    let yaml = serde_yaml::to_string(&original).expect("serialize");
    let parsed: PluginManifest = serde_yaml::from_str(&yaml).expect("deserialize");
    assert_eq!(parsed, original);
}

#[test]
fn manifest_validates_clean() {
    good_manifest().validate().expect("clean manifest");
}

#[test]
fn unknown_capability_tag_is_strict_error() {
    let yaml = r#"
name: test.plugin
version: 0.1.0
description: ""
author: ""
homepage: ""
capabilities:
  - kind: read-the-mind
plugin_type: provider
entry_point: bin/p.wasm
"#;
    let err = serde_yaml::from_str::<PluginManifest>(yaml).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("read-the-mind") || msg.contains("unknown capability"),
        "expected strict-parse error, got: {msg}"
    );
}

#[test]
fn unknown_plugin_type_is_strict_error() {
    let yaml = r#"
name: test.plugin
version: 0.1.0
description: ""
author: ""
homepage: ""
capabilities:
  - kind: extractor-confidence-override
plugin_type: telepathy
entry_point: bin/p.wasm
"#;
    serde_yaml::from_str::<PluginManifest>(yaml).unwrap_err();
}

#[test]
fn validate_catches_empty_name() {
    let mut m = good_manifest();
    m.name = String::new();
    let errs = m.validate().unwrap_err();
    assert!(errs.contains(&ValidationError::EmptyName));
}

#[test]
fn validate_catches_invalid_name() {
    let mut m = good_manifest();
    m.name = "Has Spaces!".into();
    let errs = m.validate().unwrap_err();
    assert!(errs.iter().any(|e| matches!(e, ValidationError::InvalidName(_))));
}

#[test]
fn validate_catches_invalid_version() {
    let mut m = good_manifest();
    m.version = "not-semver".into();
    let errs = m.validate().unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::InvalidVersion(_))));
}

#[test]
fn validate_catches_no_capabilities() {
    let mut m = good_manifest();
    m.capabilities.clear();
    let errs = m.validate().unwrap_err();
    assert!(errs.contains(&ValidationError::NoCapabilities));
}

#[test]
fn validate_catches_absolute_entry_point() {
    let mut m = good_manifest();
    m.entry_point = PathBuf::from("/etc/passwd");
    let errs = m.validate().unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::AbsoluteEntryPoint(_))));
}

#[test]
fn validate_catches_parent_traversal_entry_point() {
    let mut m = good_manifest();
    m.entry_point = PathBuf::from("../../etc/passwd");
    let errs = m.validate().unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::EntryPointTraversal(_))));
}

#[test]
fn install_record_marks_substrate_unsigned() {
    let m = good_manifest();
    let rec = NewInstallRecord::from_manifest(&m, "test-actor");
    assert!(!rec.signature_verified);
    assert_eq!(rec.signing_trust_root, UNSIGNED_SUBSTRATE_TRUST_ROOT);
    assert_eq!(
        rec.publisher_pubkey_fingerprint,
        UNSIGNED_SUBSTRATE_TRUST_ROOT
    );
    assert_eq!(rec.plugin_name, m.name);
    assert_eq!(rec.plugin_version, m.version);

    // capability_set is a JSON array of catalog tags
    let tags = rec.capability_set.as_array().expect("array");
    let tag_strs: Vec<&str> = tags.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(tag_strs.contains(&"network"));
    assert!(tag_strs.contains(&"read-secret"));
    assert!(tag_strs.contains(&"emit-event"));
    assert!(tag_strs.contains(&"extractor-confidence-override"));
    assert!(tag_strs.contains(&"analytics-window"));
}
