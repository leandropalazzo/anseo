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
//! DATABASE_URL=postgres://anseo:anseo@localhost:5432/anseo_test \
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

    pub fn audit(&self) -> repositories::audit::AuditRepo<'_> {
        repositories::audit::AuditRepo::new(&self.pool)
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

    /// Epic 34 — OSS claim + ground-truth storage. Premium evaluation lives in
    /// the commercial hallucination crate and depends on this surface, not the
    /// other way around.
    pub fn brand_accuracy(&self) -> repositories::brand_accuracy::BrandAccuracyRepo<'_> {
        repositories::brand_accuracy::BrandAccuracyRepo::new(&self.pool)
    }

    pub fn api_keys(&self) -> repositories::api_keys::ApiKeyRepo<'_> {
        repositories::api_keys::ApiKeyRepo::new(&self.pool)
    }

    pub fn webhook_deliveries(&self) -> repositories::webhook_deliveries::WebhookDeliveryRepo<'_> {
        repositories::webhook_deliveries::WebhookDeliveryRepo::new(&self.pool)
    }

    pub fn webhooks(&self) -> repositories::webhooks::WebhookRepo<'_> {
        repositories::webhooks::WebhookRepo::new(&self.pool)
    }

    pub fn benchmark_consent(&self) -> repositories::benchmark_consent::BenchmarkConsentRepo<'_> {
        repositories::benchmark_consent::BenchmarkConsentRepo::new(&self.pool)
    }

    /// Story 0.12 — Epic 17 GEO Recommendations substrate. No callers
    /// yet; Epic 17 stories wire up the recommender producers.
    pub fn recommendations(&self) -> repositories::recommendations::RecommendationsRepo<'_> {
        repositories::recommendations::RecommendationsRepo::new(&self.pool)
    }

    /// Story 0.12 — Epic 19 Plugin SDK install/uninstall audit log.
    pub fn plugin_installs(&self) -> repositories::plugin_installs::PluginInstallsRepo<'_> {
        repositories::plugin_installs::PluginInstallsRepo::new(&self.pool)
    }

    /// Story 31-3 — per-run provenance / lifecycle-step audit log. Written by
    /// the orchestrator write path (`persist_records`), read by
    /// `GET /runs/:id/provenance`.
    pub fn run_provenance(&self) -> repositories::run_provenance::RunProvenanceRepo<'_> {
        repositories::run_provenance::RunProvenanceRepo::new(&self.pool)
    }

    /// Epic 43 — Entity registry (domain → display-name + claim state).
    /// Also exposes dedup-review-queue operations (Story 43.3).
    pub fn entities(&self) -> repositories::entities::EntityRepo<'_> {
        repositories::entities::EntityRepo::new(&self.pool)
    }

    /// Epic 43 / Story 43.6 — Full disputes lifecycle (corrections,
    /// claim-conflict adjudication, GDPR Art.21, change-of-control, removal).
    pub fn disputes(&self) -> repositories::disputes::DisputeRepo<'_> {
        repositories::disputes::DisputeRepo::new(&self.pool)
    }

    /// Epic 43 / Story 43.2 — Domain-ownership verification (DNS-TXT challenge
    /// minting, single-use consume, rate-limit window, revocation scan).
    pub fn verification(&self) -> repositories::verification::VerificationRepo<'_> {
        repositories::verification::VerificationRepo::new(&self.pool)
    }

    /// Epic 44 / Story 44.2 — Identified-contribution persistence + server-side
    /// brand resolution (verification_token → verified domain → registry FK).
    pub fn contributions(&self) -> repositories::contributions::ContributionRepo<'_> {
        repositories::contributions::ContributionRepo::new(&self.pool)
    }

    /// Epic 47 / Story 47.1 — Privacy-safe public site-event ingest (no PII),
    /// nightly rollups, and 30-day raw retention.
    pub fn site_events(&self) -> repositories::site_events::SiteEventRepo<'_> {
        repositories::site_events::SiteEventRepo::new(&self.pool)
    }
}
