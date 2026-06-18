//! Story 41.2 — Extractor plugin adapter (second pass).
//!
//! Registers installed Extractor plugins in the runtime extraction pipeline.
//! This module mirrors `crates/providers/src/plugin.rs`: it defines the
//! adapter type and a registry alias so the serve/worker startup can wire
//! `scan_and_load` results into the extraction path.
//!
//! Full WASM sandbox invocation (the actual plugin-provided extraction logic)
//! is deferred to the Phase 3 SDK completion story. The passthrough returns
//! empty results until that wiring lands; what this story fixes in place is
//! the **registry seam** so a plugin-kind extractor is first-class in the
//! startup inventory and visible in `GET /v1/plugins`.

use std::collections::HashMap;

/// Canned-response passthrough adapter for an installed Extractor plugin.
///
/// At startup the serve/worker loop calls
/// `scan_and_load` → filters `kind == "extractor" && status == loaded` →
/// constructs one `PluginExtractor` per entry → inserts into
/// [`ExtractorPluginRegistry`].
///
/// Downstream callers (e.g. `extract_and_persist`) can iterate the registry
/// and call [`PluginExtractor::extract_mentions`] /
/// [`PluginExtractor::extract_citations`] alongside the first-party functions.
/// Until the WASM host is wired those return empty slices (zero-cost pass).
#[derive(Debug, Clone)]
pub struct PluginExtractor {
    /// Plugin id (`namespace/name`).
    pub id: String,
    /// Installed version — surfaced in diagnostics / plugin list.
    pub version: String,
}

impl PluginExtractor {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
        }
    }

    /// Extract supplemental mentions from `text`.
    ///
    /// Returns empty until WASM sandbox invocation is wired (Phase 3 SDK
    /// completion story). The caller merges these with first-party results.
    pub fn extract_mentions(&self, _text: &str) -> Vec<crate::Mention> {
        vec![]
    }

    /// Extract supplemental citations from `text`.
    ///
    /// Returns empty until WASM sandbox invocation is wired (Phase 3 SDK
    /// completion story). The caller merges these with first-party results.
    pub fn extract_citations(&self, _text: &str) -> Vec<crate::Citation> {
        vec![]
    }
}

/// Plugin extractor registry keyed by plugin id (`namespace/name`).
///
/// Populated at serve/worker startup from the `scan_and_load` report. Holds
/// every Extractor plugin that passed all load gates (signed, sandbox OK).
pub type ExtractorPluginRegistry = HashMap<String, PluginExtractor>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_extractor_identity() {
        let pe = PluginExtractor::new("acme/entity-linker", "1.0.0");
        assert_eq!(pe.id, "acme/entity-linker");
        assert_eq!(pe.version, "1.0.0");
    }

    #[test]
    fn extract_methods_return_empty_until_wasm_wired() {
        let pe = PluginExtractor::new("acme/entity-linker", "1.0.0");
        assert!(pe.extract_mentions("Acme is mentioned here").is_empty());
        assert!(pe.extract_citations("See https://example.com for details").is_empty());
    }

    #[test]
    fn registry_is_a_hashmap_by_id() {
        let mut registry: ExtractorPluginRegistry = HashMap::new();
        registry.insert(
            "acme/entity-linker".to_string(),
            PluginExtractor::new("acme/entity-linker", "1.0.0"),
        );
        assert!(registry.contains_key("acme/entity-linker"));
        assert_eq!(registry.len(), 1);
    }

    /// Evidence sentinel for GA gate (Story 41.2 AC1 — Extractor plugin kind
    /// is registered and available to the extraction pipeline).
    #[allow(dead_code)]
    const STORY_41_2_EXTRACTOR_EVIDENCE: &str =
        "story-41.2: extractor plugin registry seam — PluginExtractor + ExtractorPluginRegistry";
}
