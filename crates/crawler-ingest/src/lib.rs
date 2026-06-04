//! AI-crawler observability ingestion and metrics.
//!
//! Crawler hits are high-volume append data with source-adapter idempotency,
//! privacy-mode handling, and bot identity verification, so they live outside
//! the generic analytics crate.

pub mod adapters;
pub mod bot_identity;
pub mod metrics;
pub mod model;
pub mod sink;

pub use adapters::{
    AccessLogAdapter, AccessLogFormat, AdapterCursor, CloudFrontAdapter, CloudflareLogpushAdapter,
    CloudflareWorkersAdapter, FastlyAdapter, Ga4Adapter, IngestAdapter,
};
pub use bot_identity::{BotRangeVerifier, CidrRange};
pub use metrics::{CrawlerMetrics, MetricsParams, MetricsStore};
pub use model::{
    CrawlerIngestError, NormalizedCrawlerEvent, PrivacyMode, RawCrawlerHit, StoredClientIp,
};
pub use sink::{IngestSink, PostgresCrawlerSink};

#[cfg(feature = "clickhouse")]
pub use sink::ClickHouseCrawlerSink;
