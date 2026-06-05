//! Storage-layer error type and mapping into the workspace-wide
//! [`OpenGeoError`].
//!
//! The mapping flattens every storage failure into
//! `OpenGeoError::Internal(anyhow::Error)` — the tuple variant defined at
//! `crates/core/src/error.rs:106`. Higher layers (HTTP handlers, CLI commands)
//! decide what to surface to the caller; storage refuses to guess.

use anseo_core::OpenGeoError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("entity not found")]
    NotFound,
}

impl From<Error> for OpenGeoError {
    fn from(err: Error) -> Self {
        OpenGeoError::Internal(anyhow::anyhow!(err))
    }
}
