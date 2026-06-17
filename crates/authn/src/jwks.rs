//! JWKS key-set client (Story 21.1).
//!
//! Fetches the IdP's JSON Web Key Set from a well-known URL and caches it
//! until validation fails (standard retry-once rotation pattern).
//!
//! Key types supported: RSA (RS256 / RS384 / RS512) and EC (ES256 / ES384).
//! The `alg: none` header is unconditionally rejected (AC-2 adversarial test).

use std::sync::{Arc, RwLock};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use jsonwebtoken::{DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use crate::AuthnError;

/// A JWKS key-set fetched from an IdP's `/.well-known/jwks.json`.
#[derive(Debug, Clone)]
pub struct CachedJwks {
    pub jwk_set: JwkSet,
}

/// Async JWKS client with an in-memory cache.
///
/// Builds a `DecodingKey` from the JWKS key that matches the JWT's `kid`
/// header. If no matching key is found (could mean key rotation) the caller
/// should trigger a refresh and retry once.
#[derive(Debug, Clone)]
pub struct JwksClient {
    jwks_url: String,
    http: reqwest::Client,
    cache: Arc<RwLock<Option<JwkSet>>>,
}

impl JwksClient {
    pub fn new(jwks_url: String) -> Self {
        Self {
            jwks_url,
            http: reqwest::Client::new(),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Fetch and cache the JWKS from the IdP.
    pub async fn refresh(&self) -> Result<(), AuthnError> {
        let set: JwkSet = self
            .http
            .get(&self.jwks_url)
            .send()
            .await
            .map_err(|e| AuthnError::JwksFetch(e.to_string()))?
            .json()
            .await
            .map_err(|e| AuthnError::JwksFetch(e.to_string()))?;
        *self.cache.write().unwrap() = Some(set);
        Ok(())
    }

    /// Return a `DecodingKey` for the JWT with the given `kid`.
    /// Returns `None` if the key is not in the current cache.
    pub fn decoding_key(&self, kid: Option<&str>) -> Result<Option<DecodingKey>, AuthnError> {
        let guard = self.cache.read().unwrap();
        let Some(set) = guard.as_ref() else {
            return Ok(None);
        };

        // Find the matching JWK.
        let jwk = if let Some(kid) = kid {
            set.find(kid)
        } else {
            // No kid header — use the first available key.
            set.keys.first()
        };

        let Some(jwk) = jwk else {
            return Ok(None);
        };

        let key = match &jwk.algorithm {
            AlgorithmParameters::RSA(rsa) => DecodingKey::from_rsa_components(&rsa.n, &rsa.e)
                .map_err(|e| AuthnError::JwksFetch(e.to_string()))?,
            AlgorithmParameters::EllipticCurve(ec) => {
                let x = URL_SAFE_NO_PAD
                    .decode(&ec.x)
                    .map_err(|e| AuthnError::Malformed(format!("EC x: {e}")))?;
                let y = URL_SAFE_NO_PAD
                    .decode(&ec.y)
                    .map_err(|e| AuthnError::Malformed(format!("EC y: {e}")))?;
                DecodingKey::from_ec_components(
                    &URL_SAFE_NO_PAD.encode(&x),
                    &URL_SAFE_NO_PAD.encode(&y),
                )
                .map_err(|e| AuthnError::JwksFetch(e.to_string()))?
            }
            _ => {
                return Err(AuthnError::UnsupportedAlgorithm(
                    "unsupported JWK algorithm".into(),
                ))
            }
        };

        Ok(Some(key))
    }
}

/// Raw JWT claims shape extracted from IdP tokens.
/// Only the fields Story 21.1 requires; extras are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawClaims {
    pub sub: String,
    pub iss: String,
    #[serde(rename = "aud")]
    pub audience: serde_json::Value,
    pub exp: u64,
    pub iat: u64,
    pub org_id: Option<String>,
    pub mfa: Option<bool>,
}

/// Build a `jsonwebtoken::Validation` for a given issuer + audience.
/// Unconditionally disables `alg: none` (the library does this by default;
/// we add an explicit check in `BearerTokenAuth::validate` as belt-and-suspenders).
pub fn make_validation(issuer: &str, audience: &str) -> Validation {
    let mut v = Validation::new(jsonwebtoken::Algorithm::RS256);
    v.set_issuer(&[issuer]);
    v.set_audience(&[audience]);
    v.validate_exp = true;
    v.validate_nbf = false;
    v
}
