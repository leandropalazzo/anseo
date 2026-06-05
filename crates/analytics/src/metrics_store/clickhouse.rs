//! Phase 2 Story 14.1 — ClickHouse-backed [`MetricsStore`].
//!
//! Read-only on a small set of pre-aggregated analytics tables that the
//! ETL job in `migrate.rs` (deferred) feeds from Postgres. The trait
//! contract is FROZEN under Story 14.5's checksum guard; this impl must
//! emit byte-identical [`VisibilityPoint`] / [`CitationSummaryRow`]
//! payloads as [`postgres::PostgresMetricsStore`], modulo ordering
//! tolerances documented inline.
//!
//! Wire shape: ClickHouse's HTTP interface at `:8123` answers
//! `JSONEachRow` to GET `/?query=...`. We use `reqwest` (already in the
//! workspace tree) instead of pulling in the heavier `clickhouse` crate,
//! so the compile graph stays bounded. Auth is `Authorization: Basic`
//! over base64(`user:password`); the server is expected to be reachable
//! at `base_url`.
//!
//! Feature flag: `clickhouse`. Disabled by default — the
//! `PostgresMetricsStore` ships always; ClickHouse is opt-in
//! (architecture §3.5).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::metrics_store::{
    AnomalyRankSample, CitationSummaryRow, MetricsStore, MetricsStoreError, SummaryParams,
    TrendParams, VisibilityPoint,
};

/// Idempotent DDL the operator should apply once before the ETL fills
/// these tables. Each table is pre-aggregated to match the shape
/// `PostgresMetricsStore` emits — the trait callers can swap backends
/// without touching downstream code.
pub const SCHEMA_DDL: &str = include_str!("./clickhouse_schema.sql");

/// One pre-aggregated visibility row staged for insertion:
/// `(project_id, prompt_name, provider, bucket_start, avg_rank, presence_rate)`.
pub type VisibilityPointRow<'a> = (
    anseo_core::ProjectId,
    &'a str,
    &'a str,
    DateTime<Utc>,
    Option<f64>,
    f64,
);

#[derive(Debug, thiserror::Error)]
pub enum ClickHouseError {
    #[error("ClickHouse HTTP request failed")]
    Transport(#[from] reqwest::Error),
    #[error("ClickHouse returned non-2xx ({status}): {body}")]
    BadStatus { status: u16, body: String },
    #[error("failed to parse ClickHouse JSONEachRow response: {0}")]
    Decode(String),
}

impl From<ClickHouseError> for MetricsStoreError {
    fn from(err: ClickHouseError) -> Self {
        // The trait error currently covers Database (sqlx) + Storage
        // variants. Surface ClickHouse failures through the Storage
        // arm (it carries an opaque anseo_storage::Error which boxes
        // anyhow); this avoids a non-additive trait-shape change that
        // would break the 14.5 freeze.
        MetricsStoreError::Storage(anseo_storage::Error::Sqlx(sqlx::Error::Configuration(
            Box::new(err),
        )))
    }
}

pub struct ClickHouseMetricsStore {
    base_url: String,
    user: String,
    password: String,
    database: String,
    http: reqwest::Client,
}

impl ClickHouseMetricsStore {
    /// Construct a store. Reads creds from `CLICKHOUSE_URL`,
    /// `CLICKHOUSE_USER`, `CLICKHOUSE_PASSWORD`, `CLICKHOUSE_DATABASE`
    /// when called via [`Self::from_env`].
    pub fn new(base_url: String, user: String, password: String, database: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            user,
            password,
            database,
            http: reqwest::Client::new(),
        }
    }

    pub fn from_env() -> Result<Self, std::env::VarError> {
        Ok(Self::new(
            std::env::var("CLICKHOUSE_URL")?,
            std::env::var("CLICKHOUSE_USER")?,
            std::env::var("CLICKHOUSE_PASSWORD")?,
            std::env::var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".into()),
        ))
    }

    /// One-shot DDL applier. Splits `SCHEMA_DDL` on `;`, strips
    /// leading `--` comment lines from each chunk, and runs whatever's
    /// left as long as it's non-empty. Idempotent: every CREATE uses
    /// `IF NOT EXISTS`.
    pub async fn ensure_schema(&self) -> Result<(), ClickHouseError> {
        for raw in SCHEMA_DDL.split(';') {
            let stmt: String = raw
                .lines()
                .filter(|line| !line.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            self.execute(stmt).await?;
        }
        Ok(())
    }

    /// Convert a `ProjectId` (ULID under the hood) into the hyphenated
    /// UUID string ClickHouse's `UUID` column type accepts.
    fn pid_uuid(project_id: anseo_core::ProjectId) -> String {
        let bytes: [u8; 16] = project_id.into_ulid().to_bytes();
        uuid::Uuid::from_bytes(bytes).to_string()
    }

    /// Issue a write-side statement (INSERT, CREATE TABLE, ...).
    pub async fn execute(&self, query: &str) -> Result<(), ClickHouseError> {
        let url = format!("{}/?database={}", self.base_url, self.database);
        let response = self
            .http
            .post(&url)
            .basic_auth(&self.user, Some(&self.password))
            .body(query.to_string())
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ClickHouseError::BadStatus {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            });
        }
        Ok(())
    }

    /// Run a SELECT and deserialize each line as a `T`.
    async fn select<T: for<'de> Deserialize<'de>>(
        &self,
        sql: &str,
    ) -> Result<Vec<T>, ClickHouseError> {
        let url = format!("{}/?database={}", self.base_url, self.database);
        let response = self
            .http
            .post(&url)
            .basic_auth(&self.user, Some(&self.password))
            .body(format!("{sql} FORMAT JSONEachRow"))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(ClickHouseError::BadStatus {
                status: status.as_u16(),
                body,
            });
        }
        let mut out = Vec::new();
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parsed: T = serde_json::from_str(line)
                .map_err(|e| ClickHouseError::Decode(format!("line `{line}`: {e}")))?;
            out.push(parsed);
        }
        Ok(out)
    }

    /// Test-only seed helper. Inserts pre-aggregated visibility points.
    pub async fn seed_visibility_points(
        &self,
        rows: &[VisibilityPointRow<'_>],
    ) -> Result<(), ClickHouseError> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut sql =
            String::from("INSERT INTO visibility_points (project_id, prompt_name, provider, bucket_start, avg_rank, presence_rate) VALUES ");
        for (i, (pid, prompt, provider, ts, avg, pres)) in rows.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            let avg = match avg {
                Some(v) => format!("{v}"),
                None => "NULL".to_string(),
            };
            sql.push_str(&format!(
                "('{}','{}','{}','{}',{},{})",
                Self::pid_uuid(*pid),
                prompt.replace('\'', "''"),
                provider.replace('\'', "''"),
                ts.format("%Y-%m-%d %H:%M:%S"),
                avg,
                pres
            ));
        }
        self.execute(&sql).await
    }

    /// Test-only seed helper for citation totals.
    pub async fn seed_citation_totals(
        &self,
        rows: &[(anseo_core::ProjectId, &str, i64, Option<&str>)],
    ) -> Result<(), ClickHouseError> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut sql = String::from(
            "INSERT INTO citation_totals (project_id, domain, frequency, source_type) VALUES ",
        );
        for (i, (pid, domain, freq, source)) in rows.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            let source = match source {
                Some(s) => format!("'{}'", s.replace('\'', "''")),
                None => "NULL".to_string(),
            };
            sql.push_str(&format!(
                "('{}','{}',{},{})",
                Self::pid_uuid(*pid),
                domain.replace('\'', "''"),
                freq,
                source
            ));
        }
        self.execute(&sql).await
    }
}

#[derive(Debug, Deserialize)]
struct VisibilityPointRaw {
    bucket_epoch: i64,
    provider: String,
    avg_rank: Option<f64>,
    presence_rate: f64,
}

#[derive(Debug, Deserialize)]
struct CitationSummaryRaw {
    domain: String,
    total_frequency: String,
    top_source_type: Option<String>,
}

#[async_trait]
impl MetricsStore for ClickHouseMetricsStore {
    async fn visibility_trend(
        &self,
        project_id: anseo_core::ProjectId,
        prompt_slug: &str,
        params: TrendParams,
    ) -> Result<Vec<VisibilityPoint>, MetricsStoreError> {
        let days = params.days.clamp(1, 365);
        let escaped_slug = prompt_slug.replace('\'', "''");
        let pid_uuid = Self::pid_uuid(project_id);
        // Use the qualified column in WHERE so the SELECT-side alias
        // doesn't shadow it. Emit the bucket as a Unix timestamp —
        // ClickHouse's formatDateTime directives diverge from strftime
        // for `%M`, so an integer round-trip avoids the surprise.
        let sql = format!(
            "SELECT \
                toInt64(toUnixTimestamp(visibility_points.bucket_start)) AS bucket_epoch, \
                provider, avg_rank, presence_rate \
             FROM visibility_points \
             WHERE project_id = '{pid_uuid}' \
               AND prompt_name = '{escaped_slug}' \
               AND visibility_points.bucket_start >= now() - INTERVAL {days} DAY \
               AND visibility_points.bucket_start <= now() \
             ORDER BY visibility_points.bucket_start, provider"
        );
        let raw: Vec<VisibilityPointRaw> =
            self.select(&sql).await.map_err(MetricsStoreError::from)?;
        let mut out = Vec::with_capacity(raw.len());
        for row in raw {
            let bucket_start =
                DateTime::<Utc>::from_timestamp(row.bucket_epoch, 0).ok_or_else(|| {
                    MetricsStoreError::from(ClickHouseError::Decode(format!(
                        "bucket_epoch {} is out of DateTime range",
                        row.bucket_epoch
                    )))
                })?;
            out.push(VisibilityPoint {
                bucket_start,
                provider: row.provider,
                avg_rank: row.avg_rank,
                presence_rate: row.presence_rate,
            });
        }
        Ok(out)
    }

    async fn citation_summary(
        &self,
        project_id: anseo_core::ProjectId,
        params: SummaryParams,
    ) -> Result<Vec<CitationSummaryRow>, MetricsStoreError> {
        let limit = params.limit.clamp(1, 500);
        let pid_uuid = Self::pid_uuid(project_id);
        let sql = format!(
            "SELECT domain, \
                    toString(SUM(citation_totals.frequency)) AS total_frequency, \
                    anyHeavy(citation_totals.source_type) AS top_source_type \
             FROM citation_totals \
             WHERE project_id = '{pid_uuid}' \
             GROUP BY domain \
             ORDER BY SUM(citation_totals.frequency) DESC \
             LIMIT {limit}"
        );
        let raw: Vec<CitationSummaryRaw> =
            self.select(&sql).await.map_err(MetricsStoreError::from)?;
        let mut out = Vec::with_capacity(raw.len());
        for row in raw {
            let frequency: i64 = row.total_frequency.parse().map_err(|e| {
                MetricsStoreError::from(ClickHouseError::Decode(format!("total_frequency: {e}")))
            })?;
            out.push(CitationSummaryRow {
                domain: row.domain,
                frequency,
                source_type: row.top_source_type,
            });
        }
        Ok(out)
    }

    async fn anomaly_samples(
        &self,
        _project_id: anseo_core::ProjectId,
        _prompt_slug: &str,
        _days: i32,
    ) -> Result<Vec<AnomalyRankSample>, MetricsStoreError> {
        // Mirrors PostgresMetricsStore's deferred behaviour. The ETL
        // path for the per-(provider, observed_at) sample stream lands
        // when the orchestrator's rank-per-run output is wired in.
        Ok(Vec::new())
    }

    fn backend(&self) -> &'static str {
        "clickhouse"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clickhouse_backend_string_is_stable() {
        let store = ClickHouseMetricsStore::new(
            "http://localhost:8123".into(),
            "u".into(),
            "p".into(),
            "db".into(),
        );
        assert_eq!(store.backend(), "clickhouse");
    }
}
