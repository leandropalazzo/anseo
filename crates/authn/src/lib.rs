//! Story 21.1 — `BearerTokenAuth` behind the `Authentication` trait + JWKS validation.
//!
//! Architecture (D-P4-1):
//!   - `Authentication` trait: pluggable auth backend, returns `AuthClaims`.
//!   - `ApiKeyAuth`: existing path, untouched (self-host stays green).
//!   - `BearerTokenAuth`: validates IdP JWTs against JWKS; resolves
//!     `{operator_id, org_id, mfa}` claims from the token payload.
//!
//! JWKS key rotation: `JwksClient` caches the JWKS until a validation failure,
//! then forces a refresh (standard retry-once pattern for key rotation).
//!
//! AC-4 / RR-Phase4-MockNotGaBit: the GA bit for [p4-authn-1] requires live/
//! emulated JWKS evidence. The unit tests cover the adversarial battery with
//! constructed keys (mock-OK). The live half is exercised by the OIDC integration
//! environment (Story 21.2).

pub mod bearer;
pub mod error;
pub mod invites;
pub mod jwks;
pub mod totp;

pub use bearer::{BearerTokenAuth, TokenClaims};
pub use error::AuthnError;
pub use jwks::JwksClient;

/// The authentication outcome returned by any `Authentication` impl.
/// Resolves to the caller's identity and the org context it may act in.
#[derive(Debug, Clone)]
pub struct AuthClaims {
    /// IdP subject (`sub` claim) or API key identifier.
    pub operator_id: String,
    /// The org context this caller is authorized to act in.
    /// For `ApiKeyAuth`, resolved from the project's `org_id`.
    /// For `BearerTokenAuth`, resolved from the `org_id` JWT claim.
    pub org_id: Option<uuid::Uuid>,
    /// True when the `mfa` claim is present and `true`.
    pub mfa_verified: bool,
}

/// Pluggable authentication backend.
pub trait Authentication: Send + Sync {
    /// Validate the raw credential and return claims, or reject with an error.
    /// The credential is whatever the middleware extracted from the request
    /// (raw API key string or raw `Bearer <token>` value without the scheme).
    fn authenticate(&self, credential: &str) -> Result<AuthClaims, AuthnError>;
}
