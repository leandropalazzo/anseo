//! Story 17.4 `[plg-1]` — paired declared-allows / undeclared-fails matrix over
//! the closed §6.1 capability catalog, plus the §6.4 breaking-upgrade refusal.
//! Every refusal is a structured [`CapabilityViolation`], not a panic.

use anseo_plugin_host::capability::{
    new_capabilities, upgrade_plan, CapabilitySet, CapabilityViolation, HostAction, UpgradeRefused,
};
use anseo_plugin_manifest::Capability;

fn set(caps: Vec<Capability>) -> CapabilitySet {
    CapabilitySet::new(caps)
}

// ---- network ----
#[test]
fn network_declared_allows_listed_host_undeclared_and_offlist_fail() {
    let declared = set(vec![Capability::Network {
        allowlist: vec!["api.priya.dev".into()],
    }]);
    assert!(declared
        .check(&HostAction::HttpFetch {
            host: "api.priya.dev"
        })
        .is_ok());
    assert_eq!(
        declared.check(&HostAction::HttpFetch {
            host: "evil.example.com"
        }),
        Err(CapabilityViolation::HostNotAllowed(
            "evil.example.com".into()
        ))
    );

    let undeclared = set(vec![]);
    assert_eq!(
        undeclared.check(&HostAction::HttpFetch {
            host: "api.priya.dev"
        }),
        Err(CapabilityViolation::NetworkNotDeclared)
    );
}

// ---- read-secret ----
#[test]
fn read_secret_declared_allows_listed_id_undeclared_and_offlist_fail() {
    let declared = set(vec![Capability::ReadSecret {
        keys: vec!["plugin:priya.perplexity".into()],
    }]);
    assert!(declared
        .check(&HostAction::SecretRead {
            id: "plugin:priya.perplexity"
        })
        .is_ok());
    assert_eq!(
        declared.check(&HostAction::SecretRead { id: "user:openai" }),
        Err(CapabilityViolation::SecretNotAllowed("user:openai".into()))
    );

    let undeclared = set(vec![]);
    assert_eq!(
        undeclared.check(&HostAction::SecretRead { id: "user:openai" }),
        Err(CapabilityViolation::ReadSecretNotDeclared)
    );
}

// ---- emit-event ----
#[test]
fn emit_event_declared_allows_listed_kind_undeclared_and_offlist_fail() {
    let declared = set(vec![Capability::EmitEvent {
        kinds: vec!["citation.extracted".into()],
    }]);
    assert!(declared
        .check(&HostAction::EmitEvent {
            kind: "citation.extracted"
        })
        .is_ok());
    assert_eq!(
        declared.check(&HostAction::EmitEvent {
            kind: "secret.read"
        }),
        Err(CapabilityViolation::EventKindNotAllowed(
            "secret.read".into()
        ))
    );

    let undeclared = set(vec![]);
    assert_eq!(
        undeclared.check(&HostAction::EmitEvent {
            kind: "citation.extracted"
        }),
        Err(CapabilityViolation::EmitEventNotDeclared)
    );
}

// ---- extractor-confidence-override ----
#[test]
fn confidence_override_declared_allows_undeclared_fails() {
    let declared = set(vec![Capability::ExtractorConfidenceOverride]);
    assert!(declared.check(&HostAction::SetConfidence).is_ok());

    let undeclared = set(vec![]);
    assert_eq!(
        undeclared.check(&HostAction::SetConfidence),
        Err(CapabilityViolation::ConfidenceOverrideNotDeclared)
    );
}

// ---- analytics-window ----
#[test]
fn analytics_window_declared_narrows_undeclared_fails() {
    let declared = set(vec![Capability::AnalyticsWindow {
        windows: vec!["30d".into(), "90d".into()],
    }]);
    assert!(declared
        .check(&HostAction::AnalyticsWindow { days: 90 })
        .is_ok());
    assert!(declared
        .check(&HostAction::AnalyticsWindow { days: 7 })
        .is_ok());
    assert_eq!(
        declared.check(&HostAction::AnalyticsWindow { days: 365 }),
        Err(CapabilityViolation::WindowTooWide {
            requested: 365,
            max: 90
        })
    );

    let undeclared = set(vec![]);
    assert_eq!(
        undeclared.check(&HostAction::AnalyticsWindow { days: 30 }),
        Err(CapabilityViolation::AnalyticsWindowNotDeclared)
    );
}

// ---- §6.4 breaking-upgrade refusal ----
#[test]
fn capability_widening_upgrade_refused_without_accept_flag() {
    let old = set(vec![]);
    let new = set(vec![Capability::Network {
        allowlist: vec!["api.priya.dev".into()],
    }]);

    assert_eq!(
        new_capabilities(&old, &new),
        vec!["network:api.priya.dev".to_string()]
    );
    assert_eq!(
        upgrade_plan(&old, &new, false),
        Err(UpgradeRefused {
            added: vec!["network:api.priya.dev".into()]
        })
    );
    // With the explicit accept flag the upgrade proceeds.
    assert!(upgrade_plan(&old, &new, true).is_ok());
}

#[test]
fn non_widening_upgrade_is_not_breaking() {
    // Same grants (order-insensitive) → no new capabilities → no flag needed.
    let old = set(vec![
        Capability::Network {
            allowlist: vec!["a.com".into(), "b.com".into()],
        },
        Capability::ExtractorConfidenceOverride,
    ]);
    let new = set(vec![
        Capability::ExtractorConfidenceOverride,
        Capability::Network {
            allowlist: vec!["b.com".into(), "a.com".into()],
        },
    ]);
    assert!(new_capabilities(&old, &new).is_empty());
    assert!(upgrade_plan(&old, &new, false).is_ok());
}

#[test]
fn dropping_a_capability_is_not_a_breaking_upgrade() {
    let old = set(vec![Capability::Network {
        allowlist: vec!["a.com".into()],
    }]);
    let new = set(vec![]);
    assert!(new_capabilities(&old, &new).is_empty());
    assert!(upgrade_plan(&old, &new, false).is_ok());
}
