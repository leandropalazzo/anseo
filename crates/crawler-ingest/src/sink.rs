use async_trait::async_trait;
use opengeo_core::ProjectId;
use sqlx::PgPool;

use crate::model::{CrawlerIngestError, NormalizedCrawlerEvent};

#[async_trait]
pub trait IngestSink {
    async fn insert_events(
        &self,
        events: &[NormalizedCrawlerEvent],
    ) -> Result<u64, CrawlerIngestError>;
}

pub struct PostgresCrawlerSink {
    pool: PgPool,
}

impl PostgresCrawlerSink {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn from_storage(storage: &opengeo_storage::Storage) -> Self {
        Self::new(storage.pool().clone())
    }
}

#[async_trait]
impl IngestSink for PostgresCrawlerSink {
    async fn insert_events(
        &self,
        events: &[NormalizedCrawlerEvent],
    ) -> Result<u64, CrawlerIngestError> {
        let mut inserted = 0;
        for event in events {
            let result = sqlx::query(
                r#"
                INSERT INTO crawler_events (
                    project_id, ts, bot_id, path, status, source_adapter, raw_event_id,
                    ip_verified, region, client_ip, client_ip_truncated,
                    client_ip_hash, privacy_mode
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                ON CONFLICT (source_adapter, raw_event_id) DO NOTHING
                "#,
            )
            .bind(project_uuid(event.project_id))
            .bind(event.ts)
            .bind(&event.bot_id)
            .bind(&event.path)
            .bind(i32::from(event.status))
            .bind(&event.source_adapter)
            .bind(&event.raw_event_id)
            .bind(event.ip_verified)
            .bind(event.region.as_deref())
            .bind(event.client_ip.raw_column())
            .bind(event.client_ip.truncated_column())
            .bind(event.client_ip.hash_column())
            .bind(format!("{:?}", event.privacy_mode).to_ascii_lowercase())
            .execute(&self.pool)
            .await?;
            inserted += result.rows_affected();
        }
        Ok(inserted)
    }
}

fn project_uuid(project_id: ProjectId) -> uuid::Uuid {
    uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes())
}

#[cfg(feature = "clickhouse")]
pub struct ClickHouseCrawlerSink {
    base_url: String,
    user: String,
    password: String,
    database: String,
    http: reqwest::Client,
}

#[cfg(feature = "clickhouse")]
impl ClickHouseCrawlerSink {
    pub const SCHEMA_DDL: &'static str = r#"
        CREATE TABLE IF NOT EXISTS crawler_events (
            project_id UUID,
            ts DateTime64(3, 'UTC'),
            bot_id LowCardinality(String),
            path String,
            status UInt16,
            source_adapter LowCardinality(String),
            raw_event_id String,
            ip_verified Bool,
            region Nullable(String),
            client_ip Nullable(String),
            client_ip_truncated Nullable(String),
            client_ip_hash Nullable(String),
            privacy_mode LowCardinality(String),
            event_key String,
            inserted_at DateTime64(3, 'UTC') DEFAULT now64(3)
        )
        ENGINE = ReplacingMergeTree(inserted_at)
        ORDER BY (source_adapter, raw_event_id);
    "#;

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

    pub async fn ensure_schema(&self) -> Result<(), CrawlerIngestError> {
        self.execute(Self::SCHEMA_DDL).await
    }

    async fn execute(&self, sql: &str) -> Result<(), CrawlerIngestError> {
        let url = format!("{}/?database={}", self.base_url, self.database);
        let response = self
            .http
            .post(&url)
            .basic_auth(&self.user, Some(&self.password))
            .body(sql.to_string())
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(CrawlerIngestError::ClickHouseStatus {
                status: status.as_u16(),
                body: response.text().await.unwrap_or_default(),
            });
        }
        Ok(())
    }
}

#[cfg(feature = "clickhouse")]
#[async_trait]
impl IngestSink for ClickHouseCrawlerSink {
    async fn insert_events(
        &self,
        events: &[NormalizedCrawlerEvent],
    ) -> Result<u64, CrawlerIngestError> {
        if events.is_empty() {
            return Ok(0);
        }
        let mut sql = String::from(
            "INSERT INTO crawler_events (project_id, ts, bot_id, path, status, source_adapter, raw_event_id, ip_verified, region, client_ip, client_ip_truncated, client_ip_hash, privacy_mode, event_key) VALUES ",
        );
        for (i, event) in events.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            let event_key = format!("{}:{}", event.source_adapter, event.raw_event_id);
            sql.push_str(&format!(
                "('{}','{}','{}','{}',{},'{}','{}',{}, {}, {}, {}, {}, '{}','{}')",
                project_uuid(event.project_id),
                event.ts.format("%Y-%m-%d %H:%M:%S%.3f"),
                esc(&event.bot_id),
                esc(&event.path),
                event.status,
                esc(&event.source_adapter),
                esc(&event.raw_event_id),
                if event.ip_verified { 1 } else { 0 },
                nullable(event.region.as_deref()),
                nullable(event.client_ip.raw_column()),
                nullable(event.client_ip.truncated_column()),
                nullable(event.client_ip.hash_column()),
                format!("{:?}", event.privacy_mode).to_ascii_lowercase(),
                esc(&event_key),
            ));
        }
        self.execute(&sql).await?;
        Ok(events.len() as u64)
    }
}

#[cfg(feature = "clickhouse")]
fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "''")
}

#[cfg(feature = "clickhouse")]
fn nullable(v: Option<&str>) -> String {
    match v {
        Some(v) => format!("'{}'", esc(v)),
        None => "NULL".to_string(),
    }
}
