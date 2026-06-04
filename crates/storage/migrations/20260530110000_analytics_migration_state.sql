-- Story 0.1 — resumable ClickHouse ETL checkpoint table (architecture-phase3 §3.3 / D-8, ARCH-19).
-- One row per project records the Postgres→ClickHouse ETL progress so an
-- interrupted `ogeo analytics migrate-to-clickhouse` resumes from the last
-- completed batch instead of restarting. `finished_at IS NULL` marks an
-- in-flight (resumable) run; a non-null `finished_at` marks a clean
-- completion (the next run starts fresh).
CREATE TABLE IF NOT EXISTS analytics_migration_state (
    project_id              UUID PRIMARY KEY,
    last_completed_batch_id BIGINT      NOT NULL DEFAULT 0,
    batch_size              INTEGER     NOT NULL,
    total_rows_estimate     BIGINT      NOT NULL DEFAULT 0,
    last_heartbeat_at       TIMESTAMPTZ,
    started_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at             TIMESTAMPTZ
);
