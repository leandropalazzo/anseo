//! Closed enumeration of the four plugin kinds defined in
//! `architecture-phase3-plugin-sdk.md` §2.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// The plugin kinds supported by the Phase 3 SDK. The arch doc fixes this set
/// to four; adding a fifth requires a ratified amendment, not a serde default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    Provider,
    Extractor,
    Analytics,
    OutputFormat,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error(
    "unknown plugin type: `{0}` (expected one of: provider, extractor, analytics, output-format)"
)]
pub struct PluginTypeParseError(pub String);

impl FromStr for PluginType {
    type Err = PluginTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "provider" => Ok(PluginType::Provider),
            "extractor" => Ok(PluginType::Extractor),
            "analytics" => Ok(PluginType::Analytics),
            "output-format" => Ok(PluginType::OutputFormat),
            other => Err(PluginTypeParseError(other.to_string())),
        }
    }
}

impl fmt::Display for PluginType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PluginType::Provider => "provider",
            PluginType::Extractor => "extractor",
            PluginType::Analytics => "analytics",
            PluginType::OutputFormat => "output-format",
        };
        f.write_str(s)
    }
}
