//! Shared foundation crate for OpenGEO: errors, IDs, telemetry, secrets.
//!
//! # Stable contracts
//!
//! - **CLI exit codes** — see [`error::ExitCode`]. PRD §11.4. Stable within a major version.
//! - **Provider error taxonomy** — see [`error::ProviderErrorKind`]. PRD §11.5. Closed enum in Phase 1.
//! - **Telemetry field names** — see [`telemetry::fields`]. NFR-Observability. Renaming is breaking.
//! - **Stable IDs** — see [`ids`]. ULID-backed newtypes for Project, Prompt, PromptRun,
//!   Mention, Citation, and per-request correlation.
//!
//! # Secret handling
//!
//! API keys and other in-memory secrets MUST be held in [`secret::Secret`], which redacts
//! in `Debug`/`Display` and refuses to `Serialize`. See NFR-6 (Privacy-by-default).

pub mod config;
pub mod error;
pub mod ids;
pub mod secret;
pub mod secret_store;
pub mod telemetry;

pub use config::{
    BrandConfig, CompetitorConfig, Config, ConfigError, PromptConfig, ProviderConfig, ProviderName,
    SCHEMA_VERSION_V0_1,
};
pub use error::{ExitCode, OpenGeoError, ProviderErrorKind};
pub use ids::{CitationId, MentionId, ProjectId, PromptId, PromptRunId, RequestId};
pub use secret::Secret;
pub use secret_store::{
    default_chain, AgeFileStore, ChainedStore, InMemoryStore, KeyringStore, SecretStore,
    SecretStoreError, AGE_PASSPHRASE_ENV, KEYRING_SERVICE,
};
