//! OpenGEO storage layer: SQLx pool, forward-only migrations, and Phase 1
//! repositories.
//!
//! # Migrations
//!
//! Migrations live in `crates/storage/migrations/` as
//! `<YYYYMMDDHHMMSS>_<name>.sql` files and are forward-only (architecture
//! decision D-2). Apply them at runtime via [`Storage::migrate`], or via the
//! `sqlx` CLI:
//!
//! ```text
//! DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo_test \
//!   sqlx migrate run --source crates/storage/migrations
//! ```
//!
//! # Compile-time-checked queries
//!
//! Repositories use `sqlx::query!` / `sqlx::query_as!`, which validate SQL
//! against the live `DATABASE_URL` at compile time. To build offline (no DB
//! available), the `.sqlx/` query cache must be present. Regenerate it after
//! migrations change:
//!
//! ```text
//! DATABASE_URL=... cargo sqlx prepare --workspace -- --tests
//! ```
//!
//! Commit the resulting `.sqlx/` directory.
//!
//! # Integration testing
//!
//! Tests that touch Postgres use `#[sqlx::test]` against an ephemeral schema
//! that sqlx creates and drops per test (architecture L604). See
//! `crates/storage/tests/migration.rs` for the Story 1.3 smoke test.

pub mod error;
pub mod models;
pub mod repositories;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub use error::Error;

pub struct Storage {
    pool: PgPool,
}

impl Storage {
    /// Open a connection pool against `url`. The pool size is conservative
    /// (max 20 connections) — Phase 1 deployments are localhost-first and the
    /// API + worker share the pool.
    pub async fn connect(url: &str) -> Result<Self, Error> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    /// Construct a `Storage` from an existing `PgPool`. Useful in
    /// `#[sqlx::test]` bodies where sqlx hands the test a pre-built pool.
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Apply every embedded migration in forward order. Idempotent: previously
    /// applied migrations are skipped via the `_sqlx_migrations` table.
    pub async fn migrate(&self) -> Result<(), Error> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn projects(&self) -> repositories::projects::ProjectRepo<'_> {
        repositories::projects::ProjectRepo::new(&self.pool)
    }

    pub fn prompts(&self) -> repositories::prompts::PromptRepo<'_> {
        repositories::prompts::PromptRepo::new(&self.pool)
    }

    pub fn prompt_runs(&self) -> repositories::prompt_runs::PromptRunRepo<'_> {
        repositories::prompt_runs::PromptRunRepo::new(&self.pool)
    }

    pub fn mentions(&self) -> repositories::mentions::MentionRepo<'_> {
        repositories::mentions::MentionRepo::new(&self.pool)
    }

    pub fn citations(&self) -> repositories::citations::CitationRepo<'_> {
        repositories::citations::CitationRepo::new(&self.pool)
    }
}
