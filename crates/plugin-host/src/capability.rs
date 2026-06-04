//! Story 17.4 — capability enforcement (architecture-phase3-plugin-sdk §6).
//!
//! The closed [`Capability`](opengeo_plugin_manifest::Capability) catalog is
//! declared at install time; this module is the **call-time** half of §6.2: a
//! pure function from a declared [`CapabilitySet`] + an attempted [`HostAction`]
//! to allow / structurally-refuse. A refusal is a typed
//! [`CapabilityViolation`], never a panic — §6.2 specifies "structured error to
//! the plugin … not a sandbox escape".
//!
//! §6.4 capability-set changes between versions are a **breaking upgrade**:
//! [`upgrade_plan`] refuses any new capability without `--accept-new-capabilities`.

use opengeo_plugin_manifest::Capability;
use thiserror::Error;

/// A plugin's declared capabilities, as parsed from `plugin.toml [capabilities]`.
#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    caps: Vec<Capability>,
}

/// A host call a plugin attempts at runtime. Each maps to one §6.1 catalog row.
#[derive(Debug, Clone)]
pub enum HostAction<'a> {
    /// `host:http/fetch` to `host[:port]`.
    HttpFetch { host: &'a str },
    /// `host:secret/read` of secret id.
    SecretRead { id: &'a str },
    /// Emit an Audit Event of the given type.
    EmitEvent { kind: &'a str },
    /// Report `ExtractorResult::confidence` itself.
    SetConfidence,
    /// Request an analytics query spanning `days`.
    AnalyticsWindow { days: u32 },
}

/// A structured refusal (§6.2 "call time"). The host emits a
/// `plugin.capability.violation` audit event and returns this to the plugin.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityViolation {
    #[error("network capability not declared; `host:http/fetch` is not linked")]
    NetworkNotDeclared,
    #[error("host `{0}` is not in the declared network allowlist")]
    HostNotAllowed(String),
    #[error("read-secret capability not declared; `host:secret/read` is not linked")]
    ReadSecretNotDeclared,
    #[error("secret `{0}` is not in the declared read-secret list")]
    SecretNotAllowed(String),
    #[error("emit-event capability not declared")]
    EmitEventNotDeclared,
    #[error("event kind `{0}` is not in the declared emit-event list")]
    EventKindNotAllowed(String),
    #[error("extractor-confidence-override not declared; host computes default confidence")]
    ConfidenceOverrideNotDeclared,
    #[error("analytics-window not declared")]
    AnalyticsWindowNotDeclared,
    #[error("requested window {requested}d exceeds declared maximum {max}d")]
    WindowTooWide { requested: u32, max: u32 },
}

/// Parse a catalog window name like `"90d"` to days. Unparseable names
/// contribute 0 (i.e. never widen the allowed maximum).
fn window_days(name: &str) -> u32 {
    name.strip_suffix('d')
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(0)
}

impl CapabilitySet {
    pub fn new(caps: Vec<Capability>) -> Self {
        CapabilitySet { caps }
    }

    pub fn caps(&self) -> &[Capability] {
        &self.caps
    }

    fn network_allowlist(&self) -> Option<&[String]> {
        self.caps.iter().find_map(|c| match c {
            Capability::Network { allowlist } => Some(allowlist.as_slice()),
            _ => None,
        })
    }
    fn secret_keys(&self) -> Option<&[String]> {
        self.caps.iter().find_map(|c| match c {
            Capability::ReadSecret { keys } => Some(keys.as_slice()),
            _ => None,
        })
    }
    fn event_kinds(&self) -> Option<&[String]> {
        self.caps.iter().find_map(|c| match c {
            Capability::EmitEvent { kinds } => Some(kinds.as_slice()),
            _ => None,
        })
    }
    fn has_confidence_override(&self) -> bool {
        self.caps
            .iter()
            .any(|c| matches!(c, Capability::ExtractorConfidenceOverride))
    }
    fn analytics_max_days(&self) -> Option<u32> {
        self.caps.iter().find_map(|c| match c {
            Capability::AnalyticsWindow { windows } => {
                Some(windows.iter().map(|w| window_days(w)).max().unwrap_or(0))
            }
            _ => None,
        })
    }

    /// The §6.2 call-time check. `Ok(())` permits the call; `Err` is a routine
    /// refused-call the plugin may handle gracefully.
    pub fn check(&self, action: &HostAction<'_>) -> Result<(), CapabilityViolation> {
        match action {
            HostAction::HttpFetch { host } => match self.network_allowlist() {
                None => Err(CapabilityViolation::NetworkNotDeclared),
                Some(allow) if allow.iter().any(|h| h == host) => Ok(()),
                Some(_) => Err(CapabilityViolation::HostNotAllowed((*host).to_string())),
            },
            HostAction::SecretRead { id } => match self.secret_keys() {
                None => Err(CapabilityViolation::ReadSecretNotDeclared),
                Some(keys) if keys.iter().any(|k| k == id) => Ok(()),
                Some(_) => Err(CapabilityViolation::SecretNotAllowed((*id).to_string())),
            },
            HostAction::EmitEvent { kind } => match self.event_kinds() {
                None => Err(CapabilityViolation::EmitEventNotDeclared),
                Some(kinds) if kinds.iter().any(|k| k == kind) => Ok(()),
                Some(_) => Err(CapabilityViolation::EventKindNotAllowed(
                    (*kind).to_string(),
                )),
            },
            HostAction::SetConfidence => {
                if self.has_confidence_override() {
                    Ok(())
                } else {
                    Err(CapabilityViolation::ConfidenceOverrideNotDeclared)
                }
            }
            HostAction::AnalyticsWindow { days } => match self.analytics_max_days() {
                None => Err(CapabilityViolation::AnalyticsWindowNotDeclared),
                Some(max) if *days <= max => Ok(()),
                Some(max) => Err(CapabilityViolation::WindowTooWide {
                    requested: *days,
                    max,
                }),
            },
        }
    }
}

/// §6.4 — a capability-set upgrade is **breaking** if the new version declares
/// any capability tag absent from the old version, or widens an existing
/// allowlist/secret/event/window/confidence grant.
#[derive(Debug, Error, PartialEq, Eq)]
#[error("upgrade adds capabilities {added:?}; re-run with --accept-new-capabilities")]
pub struct UpgradeRefused {
    pub added: Vec<String>,
}

/// The set of newly-granted surface strings the new version introduces over the
/// old one. Empty ⇒ a non-breaking upgrade.
pub fn new_capabilities(old: &CapabilitySet, new: &CapabilitySet) -> Vec<String> {
    let old_grants = grant_strings(old);
    grant_strings(new)
        .into_iter()
        .filter(|g| !old_grants.contains(g))
        .collect()
}

/// Flatten a capability set to comparable per-grant strings, e.g.
/// `network:api.priya.dev`, `read-secret:plugin:priya.x`, `analytics-window:90d`.
fn grant_strings(set: &CapabilitySet) -> Vec<String> {
    let mut out = Vec::new();
    for c in set.caps() {
        match c {
            Capability::Network { allowlist } => {
                for h in allowlist {
                    out.push(format!("network:{h}"));
                }
            }
            Capability::ReadSecret { keys } => {
                for k in keys {
                    out.push(format!("read-secret:{k}"));
                }
            }
            Capability::EmitEvent { kinds } => {
                for k in kinds {
                    out.push(format!("emit-event:{k}"));
                }
            }
            Capability::ExtractorConfidenceOverride => {
                out.push("extractor-confidence-override".to_string());
            }
            Capability::AnalyticsWindow { windows } => {
                for w in windows {
                    out.push(format!("analytics-window:{w}"));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// §6.4 enforcement. Refuses unless `accept_new_capabilities` is set when the
/// new version widens the capability surface.
pub fn upgrade_plan(
    old: &CapabilitySet,
    new: &CapabilitySet,
    accept_new_capabilities: bool,
) -> Result<(), UpgradeRefused> {
    let added = new_capabilities(old, new);
    if added.is_empty() || accept_new_capabilities {
        Ok(())
    } else {
        Err(UpgradeRefused { added })
    }
}
