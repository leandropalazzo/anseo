//! Error taxonomy and CLI exit codes.
//!
//! These are stable contracts. Any change requires a major version bump.
//!
//! - [`ExitCode`] — PRD §11.4 (Phase 1+, stable).
//! - [`ProviderErrorKind`] — PRD §11.5 (closed enum, Phase 1).
//! - [`OpenGeoError`] — top-level error type. Library functions return
//!   specific errors; binaries widen at the `main` boundary and call
//!   [`OpenGeoError::exit_code`] to produce the process exit code.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

/// CLI exit codes per PRD §11.4 (Phase 1+, stable contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ExitCode {
    Success = 0,
    VisibilityCheckFailed = 1,
    ProviderError = 2,
    ConfigError = 64,
    DataError = 65,
    AuthError = 66,
    InternalError = 70,
}

impl ExitCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl From<ExitCode> for i32 {
    fn from(code: ExitCode) -> Self {
        code as i32
    }
}

/// Provider Error Taxonomy per PRD §11.5. Closed enum for Phase 1.
///
/// Phase 2 adds `ProviderUnsupportedModel` as a backwards-compatible variant.
/// Adding it then is an additive enum extension — downstream code matching on
/// this enum will need to handle the new variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    ProviderUnauthorized,
    ProviderRateLimited,
    ProviderTimeout,
    #[serde(rename = "provider_5xx")]
    Provider5xx,
    ProviderInvalidResponse,
    NetworkError,
    // Phase 2 addition: ProviderUnsupportedModel,
}

impl ProviderErrorKind {
    /// Stable `snake_case` wire string. Identical to the serde rename output.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::ProviderUnauthorized => "provider_unauthorized",
            Self::ProviderRateLimited => "provider_rate_limited",
            Self::ProviderTimeout => "provider_timeout",
            Self::Provider5xx => "provider_5xx",
            Self::ProviderInvalidResponse => "provider_invalid_response",
            Self::NetworkError => "network_error",
        }
    }
}

impl std::fmt::Display for ProviderErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

/// Top-level OpenGEO error. Every binary's `main` widens specific library
/// errors into this enum and calls [`OpenGeoError::exit_code`] to produce
/// the process exit code.
#[derive(Debug, Error)]
pub enum OpenGeoError {
    #[error("provider error: {kind} ({message})")]
    Provider {
        kind: ProviderErrorKind,
        message: String,
    },

    #[error("config error: {0}")]
    Config(String),

    #[error("data error: {0}")]
    Data(String),

    #[error("auth error: {0}")]
    Auth(String),

    #[error("visibility check failed: {0}")]
    VisibilityCheckFailed(String),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl OpenGeoError {
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::VisibilityCheckFailed(_) => ExitCode::VisibilityCheckFailed,
            Self::Provider { .. } => ExitCode::ProviderError,
            Self::Config(_) => ExitCode::ConfigError,
            Self::Data(_) => ExitCode::DataError,
            Self::Auth(_) => ExitCode::AuthError,
            Self::Internal(_) => ExitCode::InternalError,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_constants_are_stable() {
        assert_eq!(ExitCode::Success as i32, 0);
        assert_eq!(ExitCode::VisibilityCheckFailed as i32, 1);
        assert_eq!(ExitCode::ProviderError as i32, 2);
        assert_eq!(ExitCode::ConfigError as i32, 64);
        assert_eq!(ExitCode::DataError as i32, 65);
        assert_eq!(ExitCode::AuthError as i32, 66);
        assert_eq!(ExitCode::InternalError as i32, 70);
    }

    #[test]
    fn error_variants_map_to_exit_codes() {
        let cases: [(OpenGeoError, ExitCode); 6] = [
            (
                OpenGeoError::VisibilityCheckFailed("under threshold".into()),
                ExitCode::VisibilityCheckFailed,
            ),
            (
                OpenGeoError::Provider {
                    kind: ProviderErrorKind::ProviderRateLimited,
                    message: "429".into(),
                },
                ExitCode::ProviderError,
            ),
            (
                OpenGeoError::Config("bad yaml".into()),
                ExitCode::ConfigError,
            ),
            (OpenGeoError::Data("corrupt".into()), ExitCode::DataError),
            (OpenGeoError::Auth("denied".into()), ExitCode::AuthError),
            (
                OpenGeoError::Internal(anyhow::anyhow!("oops")),
                ExitCode::InternalError,
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.exit_code(), expected, "wrong exit code for {err:?}");
        }
    }

    #[test]
    fn provider_error_kind_serializes_snake_case() {
        let cases = [
            (
                ProviderErrorKind::ProviderUnauthorized,
                "\"provider_unauthorized\"",
            ),
            (
                ProviderErrorKind::ProviderRateLimited,
                "\"provider_rate_limited\"",
            ),
            (ProviderErrorKind::ProviderTimeout, "\"provider_timeout\""),
            (ProviderErrorKind::Provider5xx, "\"provider_5xx\""),
            (
                ProviderErrorKind::ProviderInvalidResponse,
                "\"provider_invalid_response\"",
            ),
            (ProviderErrorKind::NetworkError, "\"network_error\""),
        ];
        for (variant, expected_json) in cases {
            let actual = serde_json::to_string(&variant).unwrap();
            assert_eq!(actual, expected_json, "wrong JSON for {variant:?}");
        }
    }

    #[test]
    fn provider_error_kind_round_trips() {
        let variants = [
            ProviderErrorKind::ProviderUnauthorized,
            ProviderErrorKind::ProviderRateLimited,
            ProviderErrorKind::ProviderTimeout,
            ProviderErrorKind::Provider5xx,
            ProviderErrorKind::ProviderInvalidResponse,
            ProviderErrorKind::NetworkError,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            let restored: ProviderErrorKind = serde_json::from_str(&json).unwrap();
            assert_eq!(variant, restored);
        }
    }

    #[test]
    fn provider_error_kind_as_wire_str_matches_serde() {
        let variants = [
            ProviderErrorKind::ProviderUnauthorized,
            ProviderErrorKind::ProviderRateLimited,
            ProviderErrorKind::ProviderTimeout,
            ProviderErrorKind::Provider5xx,
            ProviderErrorKind::ProviderInvalidResponse,
            ProviderErrorKind::NetworkError,
        ];
        for variant in variants {
            let json = serde_json::to_string(&variant).unwrap();
            // json includes surrounding quotes; trim them off.
            let unquoted = &json[1..json.len() - 1];
            assert_eq!(
                variant.as_wire_str(),
                unquoted,
                "as_wire_str drift for {variant:?}"
            );
        }
    }
}
