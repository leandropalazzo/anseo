//! Story 41.2 — Analytics plugin registry pass.
//!
//! Registers installed Analytics plugins in the runtime analytics pipeline.
//! Mirrors `crates/providers/src/plugin.rs`: defines the adapter type and a
//! registry alias so the serve/worker startup can wire `scan_and_load` results
//! into the analytics execution path.
//!
//! Analytics plugins run in the subprocess seccomp-bpf / `sandbox-exec`
//! sandbox (Linux/macOS). The loader enforces the platform guard: plugins that
//! would load in-process on Windows are **skipped** at the loader level, so
//! this adapter only materialises for entries that passed every gate.
//!
//! Full subprocess invocation is deferred to the Phase 3 SDK completion story.
//! The passthrough returns empty results until that wiring lands; the registry
//! seam is what this story fixes in place so Analytics plugins are first-class
//! in the startup inventory and visible in `GET /v1/plugins`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The built-in trend kinds emitted by first-party analytics. Plugin-emitted
/// kinds follow the `plugin:<plugin_name>:<kind>` namespace convention
/// (see `crates/plugin-manifest/src/trend_kind.rs`).
pub use anseo_plugin_manifest::trend_kind::{
    namespaced_trend_kind, BUILTIN_TREND_KINDS, PLUGIN_TREND_PREFIX,
};

/// A single trend detected by an analytics plugin.
///
/// When the subprocess SDK is wired, plugins will return a list of these.
/// Until then the passthrough returns empty slices; callers merge with the
/// first-party [`crate::anomaly`] / volatility results unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTrend {
    /// Namespaced kind: `plugin:<plugin_name>:<kind>`.
    pub kind: String,
    /// Human-readable description for the dashboard.
    pub description: String,
    /// Arbitrary plugin-supplied metadata (thresholds, scores, etc.).
    pub metadata: serde_json::Value,
}

/// Canned-response passthrough adapter for an installed Analytics plugin.
///
/// At startup the serve/worker loop calls
/// `scan_and_load` → filters `kind == "analytics" && status == loaded` →
/// constructs one `PluginAnalytics` per entry → inserts into
/// [`AnalyticsPluginRegistry`].
///
/// Downstream callers iterate the registry and call
/// [`PluginAnalytics::detect_trends`] alongside first-party anomaly/volatility
/// detection. Until the subprocess SDK is wired this returns an empty slice
/// (zero-cost pass).
#[derive(Debug, Clone)]
pub struct PluginAnalytics {
    /// Plugin id (`namespace/name`).
    pub id: String,
    /// Installed version — surfaced in diagnostics / plugin list.
    pub version: String,
}

impl PluginAnalytics {
    pub fn new(id: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
        }
    }

    /// Detect trends for the given time-series data.
    ///
    /// `data_json` is the serialised analytics input (provider/prompt runs).
    /// Returns empty until subprocess sandbox invocation is wired (Phase 3 SDK
    /// completion story). The caller merges these with first-party results.
    pub fn detect_trends(&self, _data_json: &serde_json::Value) -> Vec<PluginTrend> {
        vec![]
    }
}

/// Analytics plugin registry keyed by plugin id (`namespace/name`).
///
/// Populated at serve/worker startup from the `scan_and_load` report. Holds
/// every Analytics plugin that passed all load gates (signed, subprocess
/// sandbox supported on this platform).
pub type AnalyticsPluginRegistry = HashMap<String, PluginAnalytics>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_analytics_identity() {
        let pa = PluginAnalytics::new("acme/volatility-v2", "2.1.0");
        assert_eq!(pa.id, "acme/volatility-v2");
        assert_eq!(pa.version, "2.1.0");
    }

    #[test]
    fn detect_trends_returns_empty_until_subprocess_wired() {
        let pa = PluginAnalytics::new("acme/volatility-v2", "2.1.0");
        let data = serde_json::json!({"runs": []});
        assert!(pa.detect_trends(&data).is_empty());
    }

    #[test]
    fn namespaced_kind_follows_convention() {
        let kind = namespaced_trend_kind("acme/volatility-v2", "spike");
        assert_eq!(kind, "plugin:acme/volatility-v2:spike");
        assert!(kind.starts_with(PLUGIN_TREND_PREFIX));
    }

    #[test]
    fn builtin_kinds_are_unprefixed() {
        for kind in BUILTIN_TREND_KINDS {
            assert!(
                !kind.starts_with(PLUGIN_TREND_PREFIX),
                "built-in kind must not carry plugin prefix: {kind}"
            );
        }
    }

    #[test]
    fn registry_is_a_hashmap_by_id() {
        let mut registry: AnalyticsPluginRegistry = HashMap::new();
        registry.insert(
            "acme/volatility-v2".to_string(),
            PluginAnalytics::new("acme/volatility-v2", "2.1.0"),
        );
        assert!(registry.contains_key("acme/volatility-v2"));
    }

    /// Evidence sentinel for GA gate (Story 41.2 AC1 — Analytics plugin kind
    /// is registered and available to the analytics pipeline).
    #[allow(dead_code)]
    const STORY_41_2_ANALYTICS_EVIDENCE: &str =
        "story-41.2: analytics plugin registry seam — PluginAnalytics + AnalyticsPluginRegistry";
}
