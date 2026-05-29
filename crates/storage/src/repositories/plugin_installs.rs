//! Story 0.12 — Repository stub for the `plugin_installs` audit table
//! (Epic 19 Plugin SDK substrate). Uses the runtime `sqlx::query` form
//! for the same reason as [`super::recommendations`]: Story 0.12 ships
//! ahead of the `.sqlx/` offline cache regeneration.
//!
//! Methods carry `#[allow(dead_code)]` because Epic 19 consumers don't
//! exist yet; the substrate is in place so plugin install/uninstall
//! code can land without a separate migration.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct PluginInstallRow {
    pub id: Uuid,
    pub plugin_name: String,
    pub plugin_version: String,
    pub publisher_pubkey_fingerprint: String,
    pub installed_at: DateTime<Utc>,
    pub installed_by_actor: String,
    pub capability_set: JsonValue,
    pub signature_verified: bool,
    pub signing_trust_root: String,
    pub removed_at: Option<DateTime<Utc>>,
    pub removed_reason: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NewPluginInstall<'a> {
    pub plugin_name: &'a str,
    pub plugin_version: &'a str,
    pub publisher_pubkey_fingerprint: &'a str,
    pub installed_by_actor: &'a str,
    /// JSON array of capability strings from the closed catalog. The
    /// SDK validates the catalog membership before calling `insert`.
    pub capability_set: JsonValue,
    pub signature_verified: bool,
    pub signing_trust_root: &'a str,
}

pub struct PluginInstallsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> PluginInstallsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Record an install event. Idempotency / dedupe is the SDK's job:
    /// the audit table is append-only and welcomes multiple rows for the
    /// same `(plugin_name, plugin_version)` (e.g. uninstall → reinstall).
    #[allow(dead_code)]
    pub async fn insert(&self, install: NewPluginInstall<'_>) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"
            INSERT INTO plugin_installs
                (id, plugin_name, plugin_version, publisher_pubkey_fingerprint,
                 installed_by_actor, capability_set, signature_verified,
                 signing_trust_root)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(id)
        .bind(install.plugin_name)
        .bind(install.plugin_version)
        .bind(install.publisher_pubkey_fingerprint)
        .bind(install.installed_by_actor)
        .bind(install.capability_set)
        .bind(install.signature_verified)
        .bind(install.signing_trust_root)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Currently-active installs (i.e. `removed_at IS NULL`), most
    /// recent first. The SDK calls this on startup to rebuild its
    /// in-memory plugin registry.
    #[allow(dead_code)]
    pub async fn find_active(&self) -> Result<Vec<PluginInstallRow>, Error> {
        let rows = sqlx::query(
            r#"
            SELECT id, plugin_name, plugin_version, publisher_pubkey_fingerprint,
                   installed_at, installed_by_actor, capability_set,
                   signature_verified, signing_trust_root, removed_at,
                   removed_reason
            FROM plugin_installs
            WHERE removed_at IS NULL
            ORDER BY installed_at DESC
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        rows.into_iter().map(row_to_install).collect()
    }

    /// Soft-remove the active install row(s) for `plugin_name`. Returns
    /// the count of rows updated (0 if nothing active). We update every
    /// active row for the name to keep the invariant "no active install
    /// for X" after the call — extra defensive given multiple versions
    /// could theoretically be active at once.
    #[allow(dead_code)]
    pub async fn mark_removed(
        &self,
        plugin_name: &str,
        reason: Option<&str>,
    ) -> Result<u64, Error> {
        let result = sqlx::query(
            r#"
            UPDATE plugin_installs
            SET removed_at = now(),
                removed_reason = $2
            WHERE plugin_name = $1
              AND removed_at IS NULL
            "#,
        )
        .bind(plugin_name)
        .bind(reason)
        .execute(self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

fn row_to_install(r: sqlx::postgres::PgRow) -> Result<PluginInstallRow, Error> {
    Ok(PluginInstallRow {
        id: r.try_get("id")?,
        plugin_name: r.try_get("plugin_name")?,
        plugin_version: r.try_get("plugin_version")?,
        publisher_pubkey_fingerprint: r.try_get("publisher_pubkey_fingerprint")?,
        installed_at: r.try_get("installed_at")?,
        installed_by_actor: r.try_get("installed_by_actor")?,
        capability_set: r.try_get("capability_set")?,
        signature_verified: r.try_get("signature_verified")?,
        signing_trust_root: r.try_get("signing_trust_root")?,
        removed_at: r.try_get("removed_at")?,
        removed_reason: r.try_get("removed_reason")?,
    })
}
