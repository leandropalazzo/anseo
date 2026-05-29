-- Phase 2 Story 14.1 — ClickHouse analytics schema.
--
-- Two pre-aggregated tables that mirror the shape `PostgresMetricsStore`
-- emits. The ETL (deferred) populates them from Postgres `prompt_runs`,
-- `mentions`, `citations`. Idempotent — every CREATE uses IF NOT EXISTS.

CREATE TABLE IF NOT EXISTS visibility_points (
    project_id      UUID,
    prompt_name     LowCardinality(String),
    provider        LowCardinality(String),
    bucket_start    DateTime,
    avg_rank        Nullable(Float64),
    presence_rate   Float64
) ENGINE = MergeTree()
ORDER BY (project_id, prompt_name, bucket_start, provider);

CREATE TABLE IF NOT EXISTS citation_totals (
    project_id      UUID,
    domain          String,
    frequency       Int64,
    source_type     Nullable(String)
) ENGINE = MergeTree()
ORDER BY (project_id, domain);
