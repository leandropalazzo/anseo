//! Trend-kind namespacing for Analytics plugins (AD-Phase3-PluginTrendKinds).
//!
//! `trend_kind` is a free-form string in the `list_trends` MCP output (it is
//! deliberately *not* a closed enum so plugins can introduce new kinds). The
//! ratified namespace convention:
//!
//! * **Built-in** kinds are unprefixed: `threshold_regression`,
//!   `statistical_anomaly`, `response_change`.
//! * **Plugin-emitted** kinds are namespaced `plugin:<plugin_name>:<kind>`.
//!
//! This module owns the one place that builds and recognises the plugin
//! namespace so the convention can't drift between the emit side (Analytics
//! plugin host) and the read side (`list_trends`).

/// Phase 3 built-in trend kinds. These are emitted unprefixed; a plugin must
/// not claim one of these names.
pub const BUILTIN_TREND_KINDS: &[&str] = &[
    "threshold_regression",
    "statistical_anomaly",
    "response_change",
];

/// The namespace prefix that marks a plugin-emitted trend kind.
pub const PLUGIN_TREND_PREFIX: &str = "plugin:";

/// Build the namespaced trend kind for a plugin-emitted trend:
/// `plugin:<plugin_name>:<kind>`.
pub fn namespaced_trend_kind(plugin_name: &str, kind: &str) -> String {
    format!("{PLUGIN_TREND_PREFIX}{plugin_name}:{kind}")
}

/// True when `trend_kind` is plugin-namespaced (i.e. not a built-in).
pub fn is_plugin_trend_kind(trend_kind: &str) -> bool {
    trend_kind.starts_with(PLUGIN_TREND_PREFIX)
}

/// True when `trend_kind` is one of the Phase 3 built-ins.
pub fn is_builtin_trend_kind(trend_kind: &str) -> bool {
    BUILTIN_TREND_KINDS.contains(&trend_kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_kind_is_namespaced() {
        let k = namespaced_trend_kind("test.analytics", "churn_spike");
        assert_eq!(k, "plugin:test.analytics:churn_spike");
        assert!(is_plugin_trend_kind(&k));
        assert!(!is_builtin_trend_kind(&k));
    }

    #[test]
    fn builtin_kinds_are_unprefixed() {
        for k in BUILTIN_TREND_KINDS {
            assert!(is_builtin_trend_kind(k));
            assert!(
                !is_plugin_trend_kind(k),
                "built-in `{k}` must not be plugin-namespaced"
            );
        }
    }
}
