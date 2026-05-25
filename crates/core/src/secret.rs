//! Secret wrapper that redacts in `Debug`/`Display` and refuses to `Serialize`.
//!
//! Thin facade over [`secrecy::SecretString`] for ergonomics:
//! - `Debug` prints `Secret([REDACTED])`.
//! - `Display` prints `[REDACTED]`.
//! - `Serialize` returns an error — secrets must never silently cross a wire
//!   or persistence boundary. Use [`Secret::expose`] and write to the OS
//!   keychain instead (architecture A-2 / OQ-20).
//! - `Deserialize` accepts a raw string (transparent), so config readers can
//!   load secrets without ceremony.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct Secret(SecretString);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(SecretString::new(value.into()))
    }

    /// Reveal the underlying secret string. Callers MUST NOT log, format, or
    /// include the returned `&str` in any structured output.
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Secret").field(&"[REDACTED]").finish()
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom(
            "Secret cannot be serialized; expose() and write to keychain instead",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "sk-very-secret-12345";

    #[test]
    fn debug_redacts() {
        let s = Secret::new(FIXTURE);
        let debug_output = format!("{s:?}");
        assert!(
            !debug_output.contains(FIXTURE),
            "Debug leaked secret: {debug_output}"
        );
        assert!(
            debug_output.contains("REDACTED"),
            "Debug missing REDACTED marker: {debug_output}"
        );
    }

    #[test]
    fn display_redacts() {
        let s = Secret::new(FIXTURE);
        assert_eq!(format!("{s}"), "[REDACTED]");
    }

    #[test]
    fn serialize_is_an_error() {
        let s = Secret::new(FIXTURE);
        let result = serde_json::to_string(&s);
        assert!(result.is_err(), "Secret must not serialize silently");
    }

    #[test]
    fn expose_returns_secret() {
        let s = Secret::new(FIXTURE);
        assert_eq!(s.expose(), FIXTURE);
    }
}
