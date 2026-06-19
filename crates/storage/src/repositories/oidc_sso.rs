//! Story 21.2 — OIDC SSO connector config repository.
//!
//! Stores per-org OIDC connector configs. Actual Cognito/AWS provisioning is
//! deferred ([mock-OK]); this layer provides the DB-backed config store so the
//! API surface and redirect flow exist for testing.
//!
//! client_secret is never stored in plaintext — callers pass a `client_secret_ref`
//! (an opaque KMS DEK-wrapped store reference). Wiring to real KMS is the
//! cloud-deferred part.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// Column tuple from `oidc_sso_connectors` RETURNING / SELECT.
type ConnectorRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    String,
    bool,
    DateTime<Utc>,
    DateTime<Utc>,
);

#[derive(Debug, Clone, serde::Serialize)]
pub struct OidcSsoConnector {
    pub id: Uuid,
    pub org_id: Uuid,
    pub provider: String,
    pub client_id: String,
    pub client_secret_ref: String,
    pub issuer_url: String,
    pub redirect_uri: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ConnectorRow> for OidcSsoConnector {
    fn from(r: ConnectorRow) -> Self {
        Self {
            id: r.0,
            org_id: r.1,
            provider: r.2,
            client_id: r.3,
            client_secret_ref: r.4,
            issuer_url: r.5,
            redirect_uri: r.6,
            enabled: r.7,
            created_at: r.8,
            updated_at: r.9,
        }
    }
}

pub struct OidcSsoRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> OidcSsoRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// List all connectors for an org. RLS requires `app.org` GUC to be set
    /// on the connection by the caller before issuing the query.
    pub async fn list_for_org(&self, org_id: Uuid) -> Result<Vec<OidcSsoConnector>, sqlx::Error> {
        let rows: Vec<ConnectorRow> = sqlx::query_as(
            r#"
            SELECT id, org_id, provider, client_id, client_secret_ref,
                   issuer_url, redirect_uri, enabled, created_at, updated_at
            FROM oidc_sso_connectors
            WHERE org_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(org_id)
        .fetch_all(self.pool)
        .await?;

        Ok(rows.into_iter().map(OidcSsoConnector::from).collect())
    }

    /// Insert a new connector record.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        org_id: Uuid,
        provider: &str,
        client_id: &str,
        client_secret_ref: &str,
        issuer_url: &str,
        redirect_uri: &str,
        enabled: bool,
    ) -> Result<OidcSsoConnector, sqlx::Error> {
        let row: ConnectorRow = sqlx::query_as(
            r#"
            INSERT INTO oidc_sso_connectors
                (org_id, provider, client_id, client_secret_ref, issuer_url, redirect_uri, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, org_id, provider, client_id, client_secret_ref,
                      issuer_url, redirect_uri, enabled, created_at, updated_at
            "#,
        )
        .bind(org_id)
        .bind(provider)
        .bind(client_id)
        .bind(client_secret_ref)
        .bind(issuer_url)
        .bind(redirect_uri)
        .bind(enabled)
        .fetch_one(self.pool)
        .await?;

        Ok(row.into())
    }

    /// Delete a connector by id, returning whether a row was actually removed.
    pub async fn delete(&self, id: Uuid, org_id: Uuid) -> Result<bool, sqlx::Error> {
        let affected = sqlx::query("DELETE FROM oidc_sso_connectors WHERE id = $1 AND org_id = $2")
            .bind(id)
            .bind(org_id)
            .execute(self.pool)
            .await?
            .rows_affected();

        Ok(affected > 0)
    }

    /// Fetch a single connector by id (for use in the redirect stub).
    pub async fn get(&self, id: Uuid) -> Result<Option<OidcSsoConnector>, sqlx::Error> {
        let row: Option<ConnectorRow> = sqlx::query_as(
            r#"
            SELECT id, org_id, provider, client_id, client_secret_ref,
                   issuer_url, redirect_uri, enabled, created_at, updated_at
            FROM oidc_sso_connectors
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(OidcSsoConnector::from))
    }
}
