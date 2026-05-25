//! Pluggable secret-storage backends for provider API keys (FR-7, FR-8, FR-11).
//!
//! Three backends ship in Phase 1:
//!
//! 1. [`KeyringStore`] — OS keychain via the `keyring` crate. Primary backend
//!    on desktop OSes (macOS Keychain, Linux Secret Service, Windows
//!    Credential Manager).
//! 2. [`AgeFileStore`] — age-passphrase-encrypted file. Fallback for headless
//!    environments (CI runners, ssh-only servers) where no Secret Service
//!    daemon is reachable. Passphrase read from `OPENGEO_KEYRING_PASSPHRASE`.
//! 3. [`InMemoryStore`] — process-local map. Used by integration tests and as
//!    a last-ditch fallback inside `ChainedStore` so a test or one-shot CI
//!    command can run without touching disk.
//!
//! [`ChainedStore`] composes these: try keyring → fall back to age-file →
//! ultimately to in-memory.
//!
//! # NFR-6 redaction posture
//!
//! Every backend stores secrets via [`Secret`]. The `expose()` call is the
//! only legal path to the raw string; callers wrap immediately on retrieval
//! (see `apps/cli/src/commands/login.rs`). `Debug` of any backend type
//! intentionally omits the underlying map.

use std::path::PathBuf;

use thiserror::Error;

use crate::Secret;

/// Service identifier used by the `keyring` backend. Stable across versions
/// so a user's stored keys survive upgrades.
pub const KEYRING_SERVICE: &str = "opengeo";

/// Environment variable that supplies the age passphrase for
/// [`AgeFileStore`].
pub const AGE_PASSPHRASE_ENV: &str = "OPENGEO_KEYRING_PASSPHRASE";

/// Errors returned by any [`SecretStore`] backend.
///
/// `Display` is one-line and never includes the secret value.
#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("backend `{backend}` reported: {message}")]
    Backend {
        backend: &'static str,
        message: String,
    },

    #[error("`{provider}` secret not found")]
    NotFound { provider: String },

    #[error("io error in `{backend}` backend: {source}")]
    Io {
        backend: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid passphrase for age-encrypted secrets file")]
    InvalidPassphrase,

    #[error("`{}` not set (required for age file fallback)", AGE_PASSPHRASE_ENV)]
    MissingPassphrase,
}

impl From<SecretStoreError> for crate::OpenGeoError {
    fn from(err: SecretStoreError) -> Self {
        crate::OpenGeoError::Auth(err.to_string())
    }
}

/// Common interface every secret backend implements.
pub trait SecretStore: Send + Sync {
    fn get(&self, provider: &str) -> Result<Secret, SecretStoreError>;
    fn set(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError>;
    /// Short human-readable backend name for logs (NEVER the secret).
    fn backend_name(&self) -> &'static str;
}

// ---------- InMemoryStore ----------

/// Process-local secret store. Used in tests and as the in-memory leg of
/// [`ChainedStore`].
#[derive(Default)]
pub struct InMemoryStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, Secret>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for InMemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryStore")
            .field("entries", &"[redacted]")
            .finish()
    }
}

impl SecretStore for InMemoryStore {
    fn get(&self, provider: &str) -> Result<Secret, SecretStoreError> {
        let guard = self.inner.lock().expect("InMemoryStore poisoned");
        guard
            .get(provider)
            .cloned()
            .ok_or_else(|| SecretStoreError::NotFound {
                provider: provider.to_string(),
            })
    }

    fn set(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError> {
        let mut guard = self.inner.lock().expect("InMemoryStore poisoned");
        guard.insert(provider.to_string(), secret);
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "in-memory"
    }
}

// ---------- KeyringStore ----------

/// Real OS-keychain backend. Available on platforms supported by the
/// `keyring` crate. Construct with [`KeyringStore::new`] — the constructor
/// itself never touches the keychain; the first `get`/`set` does.
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE.to_string(),
        }
    }

    /// Construct with an explicit service name. Used by tests that want to
    /// avoid colliding with a developer's real `opengeo` entries.
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyringStore")
            .field("service", &self.service)
            .finish()
    }
}

impl SecretStore for KeyringStore {
    fn get(&self, provider: &str) -> Result<Secret, SecretStoreError> {
        let entry = keyring::Entry::new(&self.service, provider).map_err(map_keyring)?;
        match entry.get_password() {
            Ok(s) => Ok(Secret::new(s)),
            Err(keyring::Error::NoEntry) => Err(SecretStoreError::NotFound {
                provider: provider.to_string(),
            }),
            Err(e) => Err(map_keyring(e)),
        }
    }

    fn set(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError> {
        let entry = keyring::Entry::new(&self.service, provider).map_err(map_keyring)?;
        entry.set_password(secret.expose()).map_err(map_keyring)
    }

    fn backend_name(&self) -> &'static str {
        "keyring"
    }
}

fn map_keyring(err: keyring::Error) -> SecretStoreError {
    SecretStoreError::Backend {
        backend: "keyring",
        message: err.to_string(),
    }
}

// ---------- AgeFileStore ----------

/// Age-passphrase-encrypted file backend. Stores a `{provider: secret}`
/// map serialized as JSON, then armored-age-encrypted with the passphrase in
/// `OPENGEO_KEYRING_PASSPHRASE`. The on-disk format is intentionally simple
/// so a user can `age -d` the file with their passphrase to recover keys.
///
/// Default path: `<config-dir>/opengeo/secrets.age`, where `<config-dir>` is
/// `dirs::config_dir()` (e.g. `~/.config` on Linux, `~/Library/Application
/// Support` on macOS).
pub struct AgeFileStore {
    path: PathBuf,
}

impl AgeFileStore {
    /// Construct using the default path. Returns `None` when the platform
    /// has no notion of a config directory (extremely rare).
    pub fn default_path() -> Option<Self> {
        default_secrets_path().map(|path| Self { path })
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn read_passphrase() -> Result<secrecy::SecretString, SecretStoreError> {
        let raw =
            std::env::var(AGE_PASSPHRASE_ENV).map_err(|_| SecretStoreError::MissingPassphrase)?;
        Ok(secrecy::SecretString::new(raw))
    }

    fn decrypt_map(&self) -> Result<std::collections::HashMap<String, String>, SecretStoreError> {
        if !self.path.exists() {
            return Ok(std::collections::HashMap::new());
        }
        let bytes = std::fs::read(&self.path).map_err(|e| SecretStoreError::Io {
            backend: "age-file",
            source: e,
        })?;
        let passphrase = Self::read_passphrase()?;
        let decryptor =
            match age::Decryptor::new(bytes.as_slice()).map_err(|e| SecretStoreError::Backend {
                backend: "age-file",
                message: format!("decryptor: {e}"),
            })? {
                age::Decryptor::Passphrase(d) => d,
                age::Decryptor::Recipients(_) => {
                    return Err(SecretStoreError::Backend {
                        backend: "age-file",
                        message: "secrets file uses recipient-mode age; expected passphrase mode"
                            .into(),
                    })
                }
            };
        let mut reader = decryptor
            .decrypt(&passphrase, None)
            .map_err(|_| SecretStoreError::InvalidPassphrase)?;
        let mut plaintext = Vec::new();
        use std::io::Read as _;
        reader
            .read_to_end(&mut plaintext)
            .map_err(|e| SecretStoreError::Io {
                backend: "age-file",
                source: e,
            })?;
        let map: std::collections::HashMap<String, String> = serde_json::from_slice(&plaintext)
            .map_err(|e| SecretStoreError::Backend {
                backend: "age-file",
                message: format!("decoded payload is not a JSON object: {e}"),
            })?;
        Ok(map)
    }

    fn encrypt_map(
        &self,
        map: &std::collections::HashMap<String, String>,
    ) -> Result<(), SecretStoreError> {
        let plaintext = serde_json::to_vec(map).map_err(|e| SecretStoreError::Backend {
            backend: "age-file",
            message: format!("serialize: {e}"),
        })?;
        let passphrase = Self::read_passphrase()?;
        let encryptor = age::Encryptor::with_user_passphrase(passphrase);

        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| SecretStoreError::Io {
                    backend: "age-file",
                    source: e,
                })?;
            }
        }

        let mut out = Vec::new();
        let mut writer =
            encryptor
                .wrap_output(&mut out)
                .map_err(|e| SecretStoreError::Backend {
                    backend: "age-file",
                    message: format!("wrap_output: {e}"),
                })?;
        use std::io::Write as _;
        writer
            .write_all(&plaintext)
            .map_err(|e| SecretStoreError::Io {
                backend: "age-file",
                source: e,
            })?;
        writer.finish().map_err(|e| SecretStoreError::Io {
            backend: "age-file",
            source: e,
        })?;

        // Restrictive permissions before the bytes hit disk.
        write_secrets_file(&self.path, &out)?;
        Ok(())
    }
}

impl std::fmt::Debug for AgeFileStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgeFileStore")
            .field("path", &self.path)
            .finish()
    }
}

impl SecretStore for AgeFileStore {
    fn get(&self, provider: &str) -> Result<Secret, SecretStoreError> {
        let map = self.decrypt_map()?;
        match map.get(provider) {
            Some(s) => Ok(Secret::new(s.clone())),
            None => Err(SecretStoreError::NotFound {
                provider: provider.to_string(),
            }),
        }
    }

    fn set(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError> {
        let mut map = self.decrypt_map().unwrap_or_default();
        map.insert(provider.to_string(), secret.expose().to_string());
        self.encrypt_map(&map)
    }

    fn backend_name(&self) -> &'static str {
        "age-file"
    }
}

fn default_secrets_path() -> Option<PathBuf> {
    // We intentionally do not pull in the `dirs` crate to keep dep footprint
    // small. XDG_CONFIG_HOME / HOME on Unix; %APPDATA% on Windows.
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("opengeo").join("secrets.age"))
    } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(xdg).join("opengeo").join("secrets.age"))
    } else {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("opengeo")
                .join("secrets.age")
        })
    }
}

#[cfg(unix)]
fn write_secrets_file(path: &PathBuf, bytes: &[u8]) -> Result<(), SecretStoreError> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| SecretStoreError::Io {
            backend: "age-file",
            source: e,
        })?;
    use std::io::Write as _;
    f.write_all(bytes).map_err(|e| SecretStoreError::Io {
        backend: "age-file",
        source: e,
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn write_secrets_file(path: &PathBuf, bytes: &[u8]) -> Result<(), SecretStoreError> {
    std::fs::write(path, bytes).map_err(|e| SecretStoreError::Io {
        backend: "age-file",
        source: e,
    })
}

// ---------- ChainedStore ----------

/// Tries each leg in order on `get`; writes go to the first leg that
/// accepts them.
///
/// Construct via [`default_chain`] for the CLI's standard precedence:
/// keyring → age-file → in-memory.
pub struct ChainedStore {
    legs: Vec<Box<dyn SecretStore>>,
}

impl ChainedStore {
    pub fn new(legs: Vec<Box<dyn SecretStore>>) -> Self {
        Self { legs }
    }
}

impl std::fmt::Debug for ChainedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<&str> = self.legs.iter().map(|l| l.backend_name()).collect();
        f.debug_struct("ChainedStore")
            .field("legs", &names)
            .finish()
    }
}

impl SecretStore for ChainedStore {
    fn get(&self, provider: &str) -> Result<Secret, SecretStoreError> {
        let mut last_err: Option<SecretStoreError> = None;
        for leg in &self.legs {
            match leg.get(provider) {
                Ok(s) => return Ok(s),
                Err(SecretStoreError::NotFound { .. }) => continue,
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or(SecretStoreError::NotFound {
            provider: provider.to_string(),
        }))
    }

    fn set(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError> {
        let mut last_err: Option<SecretStoreError> = None;
        for leg in &self.legs {
            match leg.set(provider, secret.clone()) {
                Ok(()) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or(SecretStoreError::Backend {
            backend: "chained",
            message: "no legs configured".into(),
        }))
    }

    fn backend_name(&self) -> &'static str {
        "chained"
    }
}

/// Default backend chain for the CLI: keyring → age-file (when default path
/// resolvable) → in-memory.
pub fn default_chain() -> ChainedStore {
    let mut legs: Vec<Box<dyn SecretStore>> = Vec::new();
    legs.push(Box::new(KeyringStore::new()));
    if let Some(file) = AgeFileStore::default_path() {
        legs.push(Box::new(file));
    }
    legs.push(Box::new(InMemoryStore::new()));
    ChainedStore::new(legs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_round_trip() {
        let store = InMemoryStore::new();
        let result = store.get("openai");
        assert!(matches!(result, Err(SecretStoreError::NotFound { .. })));

        store.set("openai", Secret::new("sk-test")).unwrap();
        assert_eq!(store.get("openai").unwrap().expose(), "sk-test");
    }

    #[test]
    fn chained_falls_through_not_found() {
        let inner_first = InMemoryStore::new();
        let inner_second = InMemoryStore::new();
        inner_second
            .set("anthropic", Secret::new("sk-anthropic"))
            .unwrap();
        let chain = ChainedStore::new(vec![Box::new(inner_first), Box::new(inner_second)]);
        assert_eq!(chain.get("anthropic").unwrap().expose(), "sk-anthropic");
    }

    #[test]
    fn debug_redacts_secret_content() {
        let store = InMemoryStore::new();
        store.set("openai", Secret::new("sk-leak-canary")).unwrap();
        let formatted = format!("{store:?}");
        assert!(
            !formatted.contains("sk-leak-canary"),
            "InMemoryStore Debug leaked secret: {formatted}"
        );
    }

    #[test]
    fn errors_never_contain_secret_value() {
        // Sanity: surfacing a NotFound on a provider we never stored should
        // not embed any "secret" string.
        let store = InMemoryStore::new();
        let err = store.get("openai").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("openai"));
        assert!(!msg.contains("REDACTED"));
    }

    #[test]
    fn age_file_round_trip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        // Use a unique passphrase env var for this test so we don't clash
        // with an ambient one — but the impl reads OPENGEO_KEYRING_PASSPHRASE
        // by name, so we just set it for the duration.
        let prior = std::env::var_os(AGE_PASSPHRASE_ENV);
        std::env::set_var(AGE_PASSPHRASE_ENV, "test-passphrase");

        let store = AgeFileStore::at(tmp.path().to_path_buf());
        store.set("openai", Secret::new("sk-fixture")).unwrap();
        store
            .set("anthropic", Secret::new("anthropic-fixture"))
            .unwrap();

        assert_eq!(store.get("openai").unwrap().expose(), "sk-fixture");
        assert_eq!(
            store.get("anthropic").unwrap().expose(),
            "anthropic-fixture"
        );
        // Make sure the on-disk file is not plaintext.
        let on_disk = std::fs::read(tmp.path()).unwrap();
        assert!(
            !String::from_utf8_lossy(&on_disk).contains("sk-fixture"),
            "secret leaked in plaintext"
        );

        match prior {
            Some(v) => std::env::set_var(AGE_PASSPHRASE_ENV, v),
            None => std::env::remove_var(AGE_PASSPHRASE_ENV),
        }
    }
}
