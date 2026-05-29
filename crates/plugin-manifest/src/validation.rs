//! Pure-data validation pass over a [`PluginManifest`].
//!
//! These checks intentionally do NOT touch the filesystem (no "does
//! entry_point exist on disk"), do NOT verify signatures, and do NOT
//! cross-check against the host SDK version. Those belong to later stories.

use crate::manifest::PluginManifest;
use std::path::{Component, Path};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("name `{0}` is not DNS-safe (allowed: a-z, 0-9, '-', '.', '_', max 128 chars)")]
    InvalidName(String),
    #[error("version `{0}` is not valid semver")]
    InvalidVersion(String),
    #[error("at least one capability declaration is required (use the explicit empty form for pure plugins)")]
    NoCapabilities,
    #[error("entry_point must not be empty")]
    EmptyEntryPoint,
    #[error("entry_point `{0}` must be a relative path")]
    AbsoluteEntryPoint(String),
    #[error("entry_point `{0}` must not contain `..` traversal components")]
    EntryPointTraversal(String),
}

impl PluginManifest {
    /// Run all pure-data checks. Returns the full list of failures so the
    /// CLI's `plugin validate` command can surface every problem at once.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errs = Vec::new();

        if self.name.is_empty() {
            errs.push(ValidationError::EmptyName);
        } else if !is_dns_safe(&self.name) {
            errs.push(ValidationError::InvalidName(self.name.clone()));
        }

        if semver::Version::parse(&self.version).is_err() {
            errs.push(ValidationError::InvalidVersion(self.version.clone()));
        }

        if self.capabilities.is_empty() {
            // Per arch §2.4, an output-format plugin may legitimately declare
            // `capabilities: []`. Substrate-only: we currently require the
            // field be present and non-empty so a missing declaration is
            // never silently treated as "no capabilities". Output-format
            // plugins can declare a single explicit zero-surface marker in a
            // later story when the host learns about it.
            errs.push(ValidationError::NoCapabilities);
        }

        validate_entry_point(&self.entry_point, &mut errs);

        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }
}

fn validate_entry_point(path: &Path, errs: &mut Vec<ValidationError>) {
    let display = path.display().to_string();
    if display.is_empty() {
        errs.push(ValidationError::EmptyEntryPoint);
        return;
    }
    if path.is_absolute() {
        errs.push(ValidationError::AbsoluteEntryPoint(display.clone()));
    }
    for comp in path.components() {
        if matches!(comp, Component::ParentDir) {
            errs.push(ValidationError::EntryPointTraversal(display));
            return;
        }
    }
}

fn is_dns_safe(name: &str) -> bool {
    if name.len() > 128 {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.' | '_'))
}
