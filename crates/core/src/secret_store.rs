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
//! # One mechanism, two key classes (Stories 36.7 + 39.1b)
//!
//! The [`SecretStore`] abstraction is shared by **two distinct key classes**,
//! each with its own namespace:
//!
//! | Key class       | Storage key shape              | Owner                             |
//! |-----------------|--------------------------------|-----------------------------------|
//! | Provider secret | `<project_id>:<provider>`      | [`provider_secret_key`] (this module) |
//! | Benchmark KEK   | `benchmark-kek:<project_id>`  | `opengeo_benchmark::kek_secret_key` |
//!
//! The namespaces are structurally disjoint: a `ProjectId` is a 26-character
//! Crockford base32 ULID; the literal `"benchmark-kek"` is 13 characters and
//! contains a hyphen, which is not valid in a ULID. Therefore a provider key
//! can **never** alias a KEK key, and vice versa — even when the same
//! `project_id` string appears in both.
//!
//! The constant [`BENCHMARK_KEK_KEY_PREFIX`] is declared here (rather than in
//! `opengeo-benchmark`) so the provider-key call site can document and enforce
//! the non-collision invariant without creating a circular dependency.
//!
//! ## Durability semantics
//!
//! - **Benchmark KEKs** are written with [`SecretStore::set_durable`], which
//!   refuses to store key material in an ephemeral (in-memory) leg. A KEK lost
//!   on restart would render every contribution it sealed permanently
//!   undecryptable, so the durable-or-fail guard is mandatory.
//! - **Provider secrets** are written with [`SecretStore::set`]. Durability is
//!   the operator's responsibility; the API does not gate on it because a lost
//!   provider key can simply be re-entered, unlike a lost KEK.
//!
//! # NFR-6 redaction posture
//!
//! Every backend stores secrets via [`Secret`]. The `expose()` call is the
//! only legal path to the raw string; callers wrap immediately on retrieval
//! (see `apps/cli/src/commands/login.rs`). `Debug` of any backend type
//! intentionally omits the underlying map.

use std::path::{Path, PathBuf};

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

    /// `set_durable` was asked to persist a secret but no durable backend
    /// (keyring or age-file) accepted the write — only an ephemeral in-memory
    /// leg was available. Surfacing this instead of silently succeeding lets
    /// callers that require durability (e.g. benchmark KEK creation) fail
    /// loudly rather than orphan key material on the next restart.
    #[error("no durable secret backend accepted the write (only ephemeral storage was available)")]
    NoDurableBackend,
}

impl From<SecretStoreError> for crate::OpenGeoError {
    fn from(err: SecretStoreError) -> Self {
        crate::OpenGeoError::Auth(err.to_string())
    }
}

// ---------- Provider-secret key namespacing (Story 36.7) ----------
//
// Provider API keys were originally keyed in ONE global namespace by the bare
// `provider` wire name (e.g. `"openai"`). Multi-project (Epic 36) needs each
// project to carry its own provider credentials, so we introduce a
// project-scoped key:
//
//     "<project_id>:<provider>"
//
// The legacy bare-`provider` key is retained as a back-compat READ fallback so
// existing single-project deployments keep resolving (see
// [`get_provider_secret`]).
//
// IMPORTANT: this namespace is intentionally DISTINCT from the benchmark
// envelope-encryption KEK namespace (Story 39.1), which keys KEKs as
// `"benchmark-kek:<project_id>"`. Provider keys lead with the raw `ProjectId`
// (a ULID), and a ULID can never equal the literal `"benchmark-kek"`, so the
// two namespaces can never alias one another. The constant below records the
// reserved benchmark prefix so this invariant is documented at the provider
// call site too.

/// Reserved key prefix owned by the benchmark KEK namespace (Story 39.1,
/// `crates/benchmark/src/crypto.rs`). Provider-secret keys MUST NOT use this
/// prefix; the project-scoped provider key shape (`<project_id>:<provider>`)
/// cannot collide with it because a `ProjectId` is a ULID and never equals the
/// literal `"benchmark-kek"`.
pub const BENCHMARK_KEK_KEY_PREFIX: &str = "benchmark-kek";

/// Build the project-scoped storage key for a provider secret.
///
/// The shape is `"<project_id>:<provider>"`. Use this everywhere a
/// project-scoped provider secret is read or written so the namespacing stays
/// consistent across the CLI, API, and worker.
pub fn provider_secret_key(project_id: &str, provider: &str) -> String {
    format!("{project_id}:{provider}")
}

/// Read a provider secret with project-scoped → legacy-global fallback.
///
/// Resolution order:
/// 1. Project-scoped key `"<project_id>:<provider>"`.
/// 2. Legacy global key `"<provider>"` (back-compat for deployments that
///    stored keys before per-project keying existed).
///
/// Returns [`SecretStoreError::NotFound`] (naming the bare `provider`) only
/// when neither key resolves. Any non-`NotFound` store error short-circuits.
pub fn get_provider_secret(
    store: &dyn SecretStore,
    project_id: &str,
    provider: &str,
) -> Result<Secret, SecretStoreError> {
    let scoped = provider_secret_key(project_id, provider);
    match store.get(&scoped) {
        Ok(s) => Ok(s),
        Err(SecretStoreError::NotFound { .. }) => store.get(provider),
        Err(other) => Err(other),
    }
}

/// Write a provider secret under the project-scoped namespace.
///
/// Writes always target the project-scoped key; the legacy global key is never
/// written (it only exists as a read fallback for pre-existing deployments).
pub fn set_provider_secret(
    store: &dyn SecretStore,
    project_id: &str,
    provider: &str,
    secret: Secret,
) -> Result<(), SecretStoreError> {
    store.set(&provider_secret_key(project_id, provider), secret)
}

/// Common interface every secret backend implements.
pub trait SecretStore: Send + Sync {
    fn get(&self, provider: &str) -> Result<Secret, SecretStoreError>;
    fn set(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError>;
    /// Delete a provider's stored secret. Idempotent: removing a provider
    /// with no stored secret returns `Ok(())`. After a successful `remove`,
    /// a subsequent `get` for the same provider returns
    /// [`SecretStoreError::NotFound`].
    fn remove(&self, provider: &str) -> Result<(), SecretStoreError>;
    /// Short human-readable backend name for logs (NEVER the secret).
    fn backend_name(&self) -> &'static str;

    /// Whether a successful `set` on this backend survives process restart.
    /// Disk/OS-keychain backends are durable; the in-memory leg is not.
    /// Defaults to `true` so any real persistent backend need not override it.
    fn is_durable(&self) -> bool {
        true
    }

    /// Like [`SecretStore::set`], but guarantees the write landed in a
    /// **durable** backend (one whose [`SecretStore::is_durable`] is `true`).
    /// Returns [`SecretStoreError::NoDurableBackend`] when the only backend
    /// able to accept the write is ephemeral.
    ///
    /// The default implementation covers single backends: persist, then check
    /// durability. [`ChainedStore`] overrides this to skip non-durable legs
    /// entirely so it never writes irrecoverable key material to memory.
    fn set_durable(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError> {
        if !self.is_durable() {
            return Err(SecretStoreError::NoDurableBackend);
        }
        self.set(provider, secret)
    }
}

// ---------- InMemoryStore ----------

/// Process-local secret store. Used in tests and as the in-memory leg of
/// [`ChainedStore`].
#[derive(Default)]
pub struct InMemoryStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, Secret>>,
    /// Process-local stores are ephemeral by definition. Tests that exercise
    /// the durable-write path without touching the real keyring/disk can opt
    /// into reporting durability via [`InMemoryStore::durable_for_tests`].
    durable: bool,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// An in-memory store that *reports* itself durable. Strictly a test aid:
    /// it lets the durable-write path (`set_durable`) be exercised without a
    /// real keyring or age-file. Never construct this in production code.
    #[doc(hidden)]
    pub fn durable_for_tests() -> Self {
        Self {
            durable: true,
            ..Self::default()
        }
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

    fn remove(&self, provider: &str) -> Result<(), SecretStoreError> {
        let mut guard = self.inner.lock().expect("InMemoryStore poisoned");
        guard.remove(provider);
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "in-memory"
    }

    fn is_durable(&self) -> bool {
        self.durable
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

    fn remove(&self, provider: &str) -> Result<(), SecretStoreError> {
        let entry = keyring::Entry::new(&self.service, provider).map_err(map_keyring)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Absent entry → idempotent success.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring(e)),
        }
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
        // age passphrase mode runs scrypt with an auto-tuned work factor
        // targeting ~1s per operation, so decrypting on every `get` would
        // make the setup-status probe (1s budget) and every run-time key
        // lookup pay that cost repeatedly. Cache the decrypted map per file,
        // keyed by mtime, so reads are ~free until the file changes.
        let current_mtime = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        if let Some(curr) = current_mtime {
            if let Ok(cache) = secrets_cache().lock() {
                if let Some((Some(cached_mtime), map)) = cache.get(&self.path) {
                    if *cached_mtime == curr {
                        return Ok(map.clone());
                    }
                }
            }
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
        store_cache(&self.path, current_mtime, &map);
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
        // Keep the in-process cache warm with what we just wrote so a
        // subsequent `get` (e.g. the status probe right after a set) skips
        // the scrypt decrypt entirely.
        let new_mtime = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .ok();
        store_cache(&self.path, new_mtime, map);
        Ok(())
    }
}

/// Process-global cache of decrypted secrets maps, keyed by file path. The
/// stored mtime lets [`AgeFileStore::decrypt_map`] detect external edits and
/// re-decrypt; a `None` mtime means "do not trust the cache for this path".
#[allow(clippy::type_complexity)]
fn secrets_cache() -> &'static std::sync::Mutex<
    std::collections::HashMap<
        PathBuf,
        (
            Option<std::time::SystemTime>,
            std::collections::HashMap<String, String>,
        ),
    >,
> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<
                PathBuf,
                (
                    Option<std::time::SystemTime>,
                    std::collections::HashMap<String, String>,
                ),
            >,
        >,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn store_cache(
    path: &Path,
    mtime: Option<std::time::SystemTime>,
    map: &std::collections::HashMap<String, String>,
) {
    if let Ok(mut cache) = secrets_cache().lock() {
        cache.insert(path.to_path_buf(), (mtime, map.clone()));
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

    fn remove(&self, provider: &str) -> Result<(), SecretStoreError> {
        let mut map = self.decrypt_map()?;
        // Absent key → nothing to rewrite; idempotent success.
        if map.remove(provider).is_none() {
            return Ok(());
        }
        // Re-encrypt and re-write the 0600 file with the entry gone.
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
    create_secrets_parent(path)?;
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
    create_secrets_parent(path)?;
    std::fs::write(path, bytes).map_err(|e| SecretStoreError::Io {
        backend: "age-file",
        source: e,
    })
}

/// Ensure the secrets file's parent directory exists. `default_secrets_path`
/// nests the file under a `opengeo/` subdirectory that may not exist yet
/// (e.g. a fresh `$XDG_CONFIG_HOME` on a headless container) — without this
/// the open would fail with `ENOENT` and the chain would silently fall back
/// to the ephemeral in-memory leg.
fn create_secrets_parent(path: &Path) -> Result<(), SecretStoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SecretStoreError::Io {
            backend: "age-file",
            source: e,
        })?;
    }
    Ok(())
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
        let mut saw_not_found = false;
        for leg in &self.legs {
            match leg.get(provider) {
                Ok(s) => return Ok(s),
                Err(SecretStoreError::NotFound { .. }) => {
                    saw_not_found = true;
                    continue;
                }
                Err(e) => last_err = Some(e),
            }
        }
        // An errored leg (e.g. an unavailable OS keyring on a headless
        // container) must not mask a genuine "absent everywhere" result: if
        // any reachable leg cleanly reported NotFound, surface NotFound so
        // callers like the provider registry skip the unkeyed provider
        // instead of aborting the whole build on the keyring's Backend error.
        if saw_not_found {
            return Err(SecretStoreError::NotFound {
                provider: provider.to_string(),
            });
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

    fn set_durable(&self, provider: &str, secret: Secret) -> Result<(), SecretStoreError> {
        // Write only to durable legs, in order, succeeding on the first that
        // accepts. The ephemeral in-memory leg is skipped entirely so this can
        // never silently persist irrecoverable key material to memory. If no
        // durable leg is present or all error, surface NoDurableBackend.
        let mut last_err: Option<SecretStoreError> = None;
        let mut saw_durable = false;
        for leg in &self.legs {
            if !leg.is_durable() {
                continue;
            }
            saw_durable = true;
            match leg.set(provider, secret.clone()) {
                Ok(()) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }
        match last_err {
            // A durable leg existed but every one errored (e.g. keyring
            // unreachable AND age-file passphrase missing): surface the real
            // backend error so the operator can fix it.
            Some(e) if saw_durable => Err(e),
            _ => Err(SecretStoreError::NoDurableBackend),
        }
    }

    fn remove(&self, provider: &str) -> Result<(), SecretStoreError> {
        // Remove from every leg so a subsequent `get` (which scans all legs)
        // can no longer find the secret. A leg with no such entry returns
        // `Ok(())` by the trait's idempotency contract. We attempt every leg
        // and, mirroring `get`/`set`, succeed if ANY leg removed cleanly —
        // a single unavailable backend (e.g. no Secret Service on a headless
        // host, where the writable age-file leg still removes) must not fail
        // the whole operation. Only when EVERY leg errors do we surface the
        // last error.
        let mut last_err: Option<SecretStoreError> = None;
        let mut any_ok = false;
        for leg in &self.legs {
            match leg.remove(provider) {
                Ok(()) => any_ok = true,
                Err(e) => last_err = Some(e),
            }
        }
        if any_ok {
            Ok(())
        } else {
            Err(last_err.unwrap_or(SecretStoreError::Backend {
                backend: "chained",
                message: "no legs configured".into(),
            }))
        }
    }

    fn backend_name(&self) -> &'static str {
        "chained"
    }

    fn is_durable(&self) -> bool {
        self.legs.iter().any(|l| l.is_durable())
    }
}

/// Environment variable that, when set to a truthy value (`1`/`true`/`yes`),
/// drops the [`KeyringStore`] leg from [`default_chain`]. Headless servers
/// (e.g. the Dockerized API) have no OS keychain; the `sync-secret-service`
/// backend can *block* trying to reach a non-existent D-Bus daemon, stalling
/// every `get`/`set` until an upstream timeout fires. Setting this makes the
/// chain go straight to the age-file (durable) and in-memory legs.
pub const DISABLE_KEYRING_ENV: &str = "OPENGEO_DISABLE_KEYRING";

fn keyring_disabled() -> bool {
    matches!(
        std::env::var(DISABLE_KEYRING_ENV).ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Default backend chain for the CLI: keyring → age-file (when default path
/// resolvable) → in-memory. On headless servers set [`DISABLE_KEYRING_ENV`]
/// to skip the keyring leg (see its docs for why).
pub fn default_chain() -> ChainedStore {
    let mut legs: Vec<Box<dyn SecretStore>> = Vec::new();
    if !keyring_disabled() {
        legs.push(Box::new(KeyringStore::new()));
    }
    if let Some(file) = AgeFileStore::default_path() {
        legs.push(Box::new(file));
    }
    legs.push(Box::new(InMemoryStore::new()));
    ChainedStore::new(legs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The age-file tests mutate the process-global `OPENGEO_KEYRING_PASSPHRASE`
    /// env var. Cargo runs tests in a binary in parallel by default, so without
    /// serialization two age tests can clobber each other's env and one sees
    /// `MissingPassphrase`. This mutex serializes any test that touches the
    /// passphrase env var.
    static AGE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn in_memory_round_trip() {
        let store = InMemoryStore::new();
        let result = store.get("openai");
        assert!(matches!(result, Err(SecretStoreError::NotFound { .. })));

        store.set("openai", Secret::new("sk-test")).unwrap();
        assert_eq!(store.get("openai").unwrap().expose(), "sk-test");
    }

    #[test]
    fn in_memory_remove_round_trip() {
        let store = InMemoryStore::new();
        store.set("openai", Secret::new("sk-test")).unwrap();
        assert_eq!(store.get("openai").unwrap().expose(), "sk-test");

        store.remove("openai").unwrap();
        assert!(matches!(
            store.get("openai"),
            Err(SecretStoreError::NotFound { .. })
        ));
    }

    #[test]
    fn in_memory_remove_absent_is_ok() {
        let store = InMemoryStore::new();
        // Removing something that was never set is a no-op success.
        store.remove("never-set").unwrap();
        assert!(matches!(
            store.get("never-set"),
            Err(SecretStoreError::NotFound { .. })
        ));
    }

    #[test]
    fn chained_remove_clears_all_legs() {
        // A provider present in more than one leg must be gone from all of
        // them after `remove`, so the chained `get` returns NotFound.
        let leg_a = InMemoryStore::new();
        let leg_b = InMemoryStore::new();
        leg_a.set("openai", Secret::new("sk-a")).unwrap();
        leg_b.set("openai", Secret::new("sk-b")).unwrap();
        let chain = ChainedStore::new(vec![Box::new(leg_a), Box::new(leg_b)]);

        assert_eq!(chain.get("openai").unwrap().expose(), "sk-a");
        chain.remove("openai").unwrap();
        assert!(matches!(
            chain.get("openai"),
            Err(SecretStoreError::NotFound { .. })
        ));
    }

    #[test]
    fn chained_remove_absent_is_ok() {
        let chain = ChainedStore::new(vec![
            Box::new(InMemoryStore::new()),
            Box::new(InMemoryStore::new()),
        ]);
        chain.remove("never-set").unwrap();
        assert!(matches!(
            chain.get("never-set"),
            Err(SecretStoreError::NotFound { .. })
        ));
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

    /// Leg that always errors with a Backend failure — models an unavailable
    /// OS keyring on a headless host (DBus/Secret-Service absent).
    struct FailingLeg;
    impl SecretStore for FailingLeg {
        fn get(&self, _provider: &str) -> Result<Secret, SecretStoreError> {
            Err(SecretStoreError::Backend {
                backend: "keyring",
                message: "secure storage unavailable".into(),
            })
        }
        fn set(&self, _provider: &str, _secret: Secret) -> Result<(), SecretStoreError> {
            Err(SecretStoreError::Backend {
                backend: "keyring",
                message: "secure storage unavailable".into(),
            })
        }
        fn remove(&self, _provider: &str) -> Result<(), SecretStoreError> {
            Err(SecretStoreError::Backend {
                backend: "keyring",
                message: "secure storage unavailable".into(),
            })
        }
        fn backend_name(&self) -> &'static str {
            "keyring"
        }
    }

    #[test]
    fn in_memory_is_not_durable_by_default() {
        let store = InMemoryStore::new();
        assert!(!store.is_durable());
        assert!(matches!(
            store.set_durable("k", Secret::new("v")),
            Err(SecretStoreError::NoDurableBackend)
        ));
        // The opt-in test variant reports durable and accepts the write.
        let durable = InMemoryStore::durable_for_tests();
        assert!(durable.is_durable());
        durable.set_durable("k", Secret::new("v")).unwrap();
        assert_eq!(durable.get("k").unwrap().expose(), "v");
    }

    #[test]
    fn chained_set_durable_requires_a_durable_leg() {
        // Only an ephemeral in-memory leg: set_durable must refuse.
        let chain = ChainedStore::new(vec![Box::new(InMemoryStore::new())]);
        assert!(!chain.is_durable());
        assert!(matches!(
            chain.set_durable("k", Secret::new("v")),
            Err(SecretStoreError::NoDurableBackend)
        ));
        // A durable leg present: set_durable writes it and skips the ephemeral
        // leg.
        let durable = InMemoryStore::durable_for_tests();
        let ephemeral = InMemoryStore::new();
        let chain = ChainedStore::new(vec![Box::new(durable), Box::new(ephemeral)]);
        assert!(chain.is_durable());
        chain.set_durable("k", Secret::new("v")).unwrap();
        assert_eq!(chain.get("k").unwrap().expose(), "v");
    }

    #[test]
    fn chained_set_durable_surfaces_backend_error_when_durable_leg_fails() {
        // A durable-looking leg that errors: the failure must surface as the
        // real backend error, not be masked as NoDurableBackend.
        let chain = ChainedStore::new(vec![Box::new(FailingLeg)]);
        assert!(matches!(
            chain.set_durable("k", Secret::new("v")),
            Err(SecretStoreError::Backend { .. })
        ));
    }

    #[test]
    fn chained_unavailable_leg_does_not_mask_not_found() {
        // keyring leg errors, age-file/in-memory leg cleanly reports absence.
        // The chain must surface NotFound (so the registry skips the provider)
        // rather than the keyring Backend error (which would abort the build).
        let chain = ChainedStore::new(vec![Box::new(FailingLeg), Box::new(InMemoryStore::new())]);
        assert!(matches!(
            chain.get("openai"),
            Err(SecretStoreError::NotFound { .. })
        ));
    }

    #[test]
    fn chained_unavailable_leg_still_returns_secret_from_later_leg() {
        let inner = InMemoryStore::new();
        inner.set("openai", Secret::new("sk-openai")).unwrap();
        let chain = ChainedStore::new(vec![Box::new(FailingLeg), Box::new(inner)]);
        assert_eq!(chain.get("openai").unwrap().expose(), "sk-openai");
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
        let _env_guard = AGE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

        // remove() drops one entry, re-writes the file, and leaves the other
        // entry intact and still retrievable.
        store.remove("openai").unwrap();
        assert!(matches!(
            store.get("openai"),
            Err(SecretStoreError::NotFound { .. })
        ));
        assert_eq!(
            store.get("anthropic").unwrap().expose(),
            "anthropic-fixture"
        );

        // Removing an absent provider is idempotent (Ok), and the surviving
        // entry is untouched.
        store.remove("openai").unwrap();
        store.remove("never-set").unwrap();
        assert_eq!(
            store.get("anthropic").unwrap().expose(),
            "anthropic-fixture"
        );

        match prior {
            Some(v) => std::env::set_var(AGE_PASSPHRASE_ENV, v),
            None => std::env::remove_var(AGE_PASSPHRASE_ENV),
        }
    }

    #[test]
    fn age_file_remove_absent_on_missing_file_is_ok() {
        // No file on disk at all: remove must still be Ok (nothing to delete)
        // and must not create a file.
        let _env_guard = AGE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.age");
        let prior = std::env::var_os(AGE_PASSPHRASE_ENV);
        std::env::set_var(AGE_PASSPHRASE_ENV, "test-passphrase");

        let store = AgeFileStore::at(path.clone());
        store.remove("openai").unwrap();
        assert!(!path.exists(), "remove should not create a secrets file");
        assert!(matches!(
            store.get("openai"),
            Err(SecretStoreError::NotFound { .. })
        ));

        match prior {
            Some(v) => std::env::set_var(AGE_PASSPHRASE_ENV, v),
            None => std::env::remove_var(AGE_PASSPHRASE_ENV),
        }
    }

    #[test]
    fn default_chain_skips_keyring_when_disabled() {
        let _env_guard = AGE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prior = std::env::var_os(DISABLE_KEYRING_ENV);

        std::env::set_var(DISABLE_KEYRING_ENV, "1");
        let legs = format!("{:?}", default_chain());
        assert!(
            !legs.contains("keyring"),
            "keyring leg should be skipped: {legs}"
        );

        std::env::remove_var(DISABLE_KEYRING_ENV);
        let legs = format!("{:?}", default_chain());
        assert!(
            legs.contains("keyring"),
            "keyring leg should be present: {legs}"
        );

        match prior {
            Some(v) => std::env::set_var(DISABLE_KEYRING_ENV, v),
            None => std::env::remove_var(DISABLE_KEYRING_ENV),
        }
    }

    #[test]
    fn default_chain_round_trip_headless_via_xdg() {
        // Mirrors the Dockerized API: keyring disabled, age-file rooted under
        // XDG_CONFIG_HOME, passphrase from env. set then get must round-trip.
        let _env_guard = AGE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let prior_xdg = std::env::var_os("XDG_CONFIG_HOME");
        let prior_pass = std::env::var_os(AGE_PASSPHRASE_ENV);
        let prior_dis = std::env::var_os(DISABLE_KEYRING_ENV);
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        std::env::set_var(AGE_PASSPHRASE_ENV, "test-passphrase");
        std::env::set_var(DISABLE_KEYRING_ENV, "1");

        use crate::SecretStore as _;
        default_chain()
            .set("openai", Secret::new("sk-headless"))
            .unwrap();
        let got = default_chain().get("openai").unwrap();
        assert_eq!(got.expose(), "sk-headless");

        let restore = |k: &str, v: Option<std::ffi::OsString>| match v {
            Some(v) => std::env::set_var(k, v),
            None => std::env::remove_var(k),
        };
        restore("XDG_CONFIG_HOME", prior_xdg);
        restore(AGE_PASSPHRASE_ENV, prior_pass);
        restore(DISABLE_KEYRING_ENV, prior_dis);
    }

    #[test]
    fn age_file_set_creates_missing_parent_dirs() {
        // `default_secrets_path` nests the file under `<config>/opengeo/`; on a
        // fresh config dir that subdirectory does not exist. `set` must create
        // it rather than erroring with ENOENT (which would silently fall the
        // chain through to the ephemeral in-memory leg).
        let _env_guard = AGE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opengeo").join("secrets.age");
        assert!(!path.parent().unwrap().exists());
        let prior = std::env::var_os(AGE_PASSPHRASE_ENV);
        std::env::set_var(AGE_PASSPHRASE_ENV, "test-passphrase");

        let store = AgeFileStore::at(path.clone());
        store.set("openai", Secret::new("sk-nested")).unwrap();
        assert!(path.exists(), "secrets file should have been created");
        assert_eq!(store.get("openai").unwrap().expose(), "sk-nested");

        match prior {
            Some(v) => std::env::set_var(AGE_PASSPHRASE_ENV, v),
            None => std::env::remove_var(AGE_PASSPHRASE_ENV),
        }
    }

    // ---------- Provider-secret keying (Story 36.7) ----------

    #[test]
    fn provider_secret_key_shape() {
        assert_eq!(provider_secret_key("proj-a", "openai"), "proj-a:openai");
    }

    #[test]
    fn provider_secret_project_scoped_isolation() {
        // Two projects store the same provider under their own namespaces; a
        // read for one project must never see the other's key.
        let store = InMemoryStore::new();
        set_provider_secret(&store, "proj-a", "openai", Secret::new("sk-a")).unwrap();
        set_provider_secret(&store, "proj-b", "openai", Secret::new("sk-b")).unwrap();

        assert_eq!(
            get_provider_secret(&store, "proj-a", "openai")
                .unwrap()
                .expose(),
            "sk-a"
        );
        assert_eq!(
            get_provider_secret(&store, "proj-b", "openai")
                .unwrap()
                .expose(),
            "sk-b"
        );
    }

    #[test]
    fn provider_secret_missing_for_unconfigured_project() {
        let store = InMemoryStore::new();
        set_provider_secret(&store, "proj-a", "openai", Secret::new("sk-a")).unwrap();
        assert!(matches!(
            get_provider_secret(&store, "proj-b", "openai"),
            Err(SecretStoreError::NotFound { .. })
        ));
    }

    #[test]
    fn provider_secret_legacy_global_fallback() {
        // A key stored under the legacy bare-provider namespace (pre-36.7) must
        // still resolve for any project via the read fallback.
        let store = InMemoryStore::new();
        store.set("openai", Secret::new("sk-legacy")).unwrap();

        assert_eq!(
            get_provider_secret(&store, "any-project", "openai")
                .unwrap()
                .expose(),
            "sk-legacy"
        );
    }

    #[test]
    fn provider_secret_scoped_wins_over_legacy() {
        // When BOTH a project-scoped key and a legacy global key exist, the
        // project-scoped one takes precedence.
        let store = InMemoryStore::new();
        store.set("openai", Secret::new("sk-legacy")).unwrap();
        set_provider_secret(&store, "proj-a", "openai", Secret::new("sk-scoped")).unwrap();

        assert_eq!(
            get_provider_secret(&store, "proj-a", "openai")
                .unwrap()
                .expose(),
            "sk-scoped"
        );
    }

    #[test]
    fn provider_and_benchmark_kek_namespaces_do_not_collide() {
        // The benchmark KEK keys as `benchmark-kek:<project_id>`; provider
        // secrets key as `<project_id>:<provider>`. Even when the same project
        // id is used, the two keys are distinct strings and writing one must
        // not affect reads of the other.
        let store = InMemoryStore::new();
        let project_id = "proj-shared";

        // Provider secret write.
        set_provider_secret(&store, project_id, "openai", Secret::new("sk-provider")).unwrap();
        // Simulate a benchmark KEK write under its own namespace.
        let kek_key = format!("{BENCHMARK_KEK_KEY_PREFIX}:{project_id}");
        store.set(&kek_key, Secret::new("kek-material")).unwrap();

        // Provider read returns the provider secret, not the KEK.
        assert_eq!(
            get_provider_secret(&store, project_id, "openai")
                .unwrap()
                .expose(),
            "sk-provider"
        );
        // KEK read returns the KEK, not the provider secret.
        assert_eq!(store.get(&kek_key).unwrap().expose(), "kek-material");

        // And the two keys are genuinely different strings.
        assert_ne!(provider_secret_key(project_id, "openai"), kek_key);
    }
}
