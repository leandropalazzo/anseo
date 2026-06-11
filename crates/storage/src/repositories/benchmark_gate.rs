//! Story 49.0 (D2) — terms-finalize gate config: OSS-owned source of truth.
//!
//! The terms-finalized toggle, the active benchmark `terms_version`, and the
//! k>=5 density floor live in OSS Postgres (`benchmark_gate_config`), NOT in
//! `anseo_admin`. An OSS consumer (CLI `anseo benchmark optin`, the ingest
//! terms-version check) reads them via [`BenchmarkGateRepo::get`] WITHOUT ever
//! touching `anseo_admin` (ADR-007). The operator console writes them via the
//! `PUT /v1/operator/config/benchmark-gate` endpoint, which upserts the single
//! sentinel row through [`BenchmarkGateRepo::upsert`].
//!
//! Dynamic sqlx only (no `query!` macros → no `.sqlx` cache regen).

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::error::Error;

/// The default k>=5 density floor. This is the build-in floor a fresh
/// deployment reports before the console first writes the gate, and matches
/// the public-benchmark k-anonymity floor used by the density reads
/// (`apps/api`'s `density_check` `contributor_count >= 5`).
pub const DEFAULT_DENSITY_FLOOR: i32 = 5;

/// The OSS-owned terms-finalize gate config (a single deployment-wide row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateConfig {
    /// Terms-finalized toggle. When false the benchmark is not-yet-open
    /// regardless of `terms_version`.
    pub terms_finalized: bool,
    /// Active benchmark terms version the optin / ingest path pins against.
    pub terms_version: String,
    /// k>=5 density floor (minimum distinct contributors per segment).
    pub density_floor: i32,
    /// Last operator to write the gate (None when never written).
    pub updated_by: Option<String>,
    /// When the gate was last written (None when never written — built-in).
    pub updated_at: Option<DateTime<Utc>>,
}

impl GateConfig {
    /// The built-in default a fresh deployment reports before the console first
    /// writes the gate: terms NOT finalized, no active version, the k=5 floor.
    pub fn builtin_default() -> Self {
        Self {
            terms_finalized: false,
            terms_version: "unset".to_string(),
            density_floor: DEFAULT_DENSITY_FLOOR,
            updated_by: None,
            updated_at: None,
        }
    }
}

pub struct BenchmarkGateRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> BenchmarkGateRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Read the gate config. Returns the persisted row, or the built-in default
    /// when the deployment has never written one (so a fresh deployment is
    /// readable). This is the read an OSS consumer (CLI / ingest) uses — it
    /// never reads `anseo_admin`.
    pub async fn get(&self) -> Result<GateConfig, Error> {
        let row = sqlx::query(
            r#"SELECT terms_finalized, terms_version, density_floor, updated_by, updated_at
               FROM benchmark_gate_config
               WHERE id = 'default'"#,
        )
        .fetch_optional(self.pool)
        .await?;
        Ok(match row {
            Some(r) => GateConfig {
                terms_finalized: r.try_get("terms_finalized")?,
                terms_version: r.try_get("terms_version")?,
                density_floor: r.try_get("density_floor")?,
                updated_by: r.try_get("updated_by")?,
                updated_at: r.try_get("updated_at")?,
            },
            None => GateConfig::builtin_default(),
        })
    }

    /// Upsert the single sentinel gate row (operator-admin write). The source of
    /// truth lives here; a subsequent [`Self::get`] reflects the new values.
    pub async fn upsert(
        &self,
        terms_finalized: bool,
        terms_version: &str,
        density_floor: i32,
        updated_by: Option<&str>,
    ) -> Result<GateConfig, Error> {
        sqlx::query(
            r#"INSERT INTO benchmark_gate_config
                   (id, terms_finalized, terms_version, density_floor, updated_by, updated_at)
               VALUES ('default', $1, $2, $3, $4, now())
               ON CONFLICT (id) DO UPDATE SET
                   terms_finalized = EXCLUDED.terms_finalized,
                   terms_version   = EXCLUDED.terms_version,
                   density_floor   = EXCLUDED.density_floor,
                   updated_by      = EXCLUDED.updated_by,
                   updated_at      = now()"#,
        )
        .bind(terms_finalized)
        .bind(terms_version)
        .bind(density_floor)
        .bind(updated_by)
        .execute(self.pool)
        .await?;
        self.get().await
    }
}
