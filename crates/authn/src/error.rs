//! Authentication error types (Story 21.1).

use thiserror::Error;

/// Authentication errors that map to HTTP 401 `auth_invalid`.
/// Each variant captures enough detail for structured logging without
/// leaking implementation details to the API caller.
#[derive(Debug, Error)]
pub enum AuthnError {
    #[error("token is expired")]
    Expired,

    #[error("token signature is invalid")]
    InvalidSignature,

    #[error("token issuer does not match expected issuer")]
    WrongIssuer,

    #[error("token audience does not match expected audience")]
    WrongAudience,

    #[error("token is missing the required org_id claim")]
    MissingOrgClaim,

    #[error("org_id claim is not a valid UUID: {0}")]
    InvalidOrgId(String),

    #[error("algorithm `none` is not permitted")]
    AlgNone,

    #[error("token uses an unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),

    #[error("token is malformed: {0}")]
    Malformed(String),

    #[error("JWKS fetch failed: {0}")]
    JwksFetch(String),

    #[error("no matching key found in JWKS for kid={0:?}")]
    NoMatchingKey(Option<String>),
}
