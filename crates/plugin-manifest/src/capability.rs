//! Closed capability catalog per arch §5/§6.1.
//!
//! Strict-parse: unknown capability tags are a hard error at manifest load
//! time. We do NOT rely on `#[serde(other)]` or default-variants; the host
//! refuses to install a plugin that declares a capability it doesn't
//! recognize so a malicious plugin can't quietly request more surface than
//! the host can enforce.

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use thiserror::Error;

/// The closed catalog. Every variant maps 1:1 to a row in
/// `architecture-phase3-plugin-sdk.md` §6.1 table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// `network:<host-allowlist>` — Provider / Analytics. The host mediates
    /// every `host:http/fetch` against this list at call time (substrate
    /// only here; enforcement lands with the WASM host).
    Network { allowlist: Vec<String> },
    /// `read-secret:<id>` — Provider. The plugin may read the named secret
    /// via `host:secret/read` (enforcement: future story).
    ReadSecret { keys: Vec<String> },
    /// `emit-event:<event-type>` — All. The plugin may emit Audit Events
    /// of the listed kinds.
    EmitEvent { kinds: Vec<String> },
    /// `extractor-confidence-override` — Extractor. Plugin may set
    /// `ExtractorResult::confidence` itself.
    ExtractorConfidenceOverride,
    /// `analytics-window` — Analytics. Maximum query window (named by the
    /// arch doc; this substrate carries the declared names through to the
    /// audit row).
    AnalyticsWindow { windows: Vec<String> },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityParseError {
    #[error("unknown capability tag: `{0}` (closed catalog: network, read-secret, emit-event, extractor-confidence-override, analytics-window)")]
    UnknownTag(String),
    #[error("capability `{tag}` requires field `{field}`")]
    MissingField { tag: String, field: String },
}

impl Capability {
    pub fn tag(&self) -> &'static str {
        match self {
            Capability::Network { .. } => "network",
            Capability::ReadSecret { .. } => "read-secret",
            Capability::EmitEvent { .. } => "emit-event",
            Capability::ExtractorConfidenceOverride => "extractor-confidence-override",
            Capability::AnalyticsWindow { .. } => "analytics-window",
        }
    }

    /// All catalog tags, in declared order. Used by validation reporting and
    /// by error messages so the user can see what's allowed.
    pub fn catalog_tags() -> &'static [&'static str] {
        &[
            "network",
            "read-secret",
            "emit-event",
            "extractor-confidence-override",
            "analytics-window",
        ]
    }
}

// ---------- Wire format ----------
//
// On disk a capability is one entry in a YAML sequence shaped like:
//
//   capabilities:
//     - kind: network
//       allowlist: ["api.example.com"]
//     - kind: read-secret
//       keys: ["plugin:priya.perplexity-pro"]
//     - kind: extractor-confidence-override
//     - kind: emit-event
//       kinds: ["citation.extracted"]
//     - kind: analytics-window
//       windows: ["30d", "90d"]
//
// We hand-roll Deserialize so we get the strict-parse guarantee: any `kind`
// outside the catalog is a hard error, not a silently-dropped variant.

impl Serialize for Capability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", self.tag())?;
        match self {
            Capability::Network { allowlist } => map.serialize_entry("allowlist", allowlist)?,
            Capability::ReadSecret { keys } => map.serialize_entry("keys", keys)?,
            Capability::EmitEvent { kinds } => map.serialize_entry("kinds", kinds)?,
            Capability::ExtractorConfidenceOverride => {}
            Capability::AnalyticsWindow { windows } => {
                map.serialize_entry("windows", windows)?
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct CapVisitor;

        impl<'de> Visitor<'de> for CapVisitor {
            type Value = Capability;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a capability map with a `kind` field from the closed catalog")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Capability, M::Error> {
                let mut kind: Option<String> = None;
                let mut allowlist: Option<Vec<String>> = None;
                let mut keys: Option<Vec<String>> = None;
                let mut kinds: Option<Vec<String>> = None;
                let mut windows: Option<Vec<String>> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => kind = Some(map.next_value()?),
                        "allowlist" => allowlist = Some(map.next_value()?),
                        "keys" => keys = Some(map.next_value()?),
                        "kinds" => kinds = Some(map.next_value()?),
                        "windows" => windows = Some(map.next_value()?),
                        other => {
                            return Err(de::Error::unknown_field(
                                other,
                                &["kind", "allowlist", "keys", "kinds", "windows"],
                            ));
                        }
                    }
                }

                let tag = kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                match tag.as_str() {
                    "network" => Ok(Capability::Network {
                        allowlist: allowlist.unwrap_or_default(),
                    }),
                    "read-secret" => Ok(Capability::ReadSecret {
                        keys: keys.unwrap_or_default(),
                    }),
                    "emit-event" => Ok(Capability::EmitEvent {
                        kinds: kinds.unwrap_or_default(),
                    }),
                    "extractor-confidence-override" => {
                        Ok(Capability::ExtractorConfidenceOverride)
                    }
                    "analytics-window" => Ok(Capability::AnalyticsWindow {
                        windows: windows.unwrap_or_default(),
                    }),
                    other => Err(de::Error::custom(format!(
                        "unknown capability tag `{other}` (closed catalog: {:?})",
                        Capability::catalog_tags()
                    ))),
                }
            }
        }

        deserializer.deserialize_map(CapVisitor)
    }
}
