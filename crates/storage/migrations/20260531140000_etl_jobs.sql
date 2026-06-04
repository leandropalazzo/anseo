-- Story 31-4 — ClickHouse ETL job queue.
--
-- The resumable Postgres→ClickHouse ETL engine
-- (`opengeo_analytics::metrics_store::clickhouse_etl::migrate_project_resumable`)
-- is behind the analytics `clickhouse` Cargo feature and can only be linked by
-- the worker (the API can't pull in reqwest there). This table is the enqueue
-- seam: 31-5's setup handler INSERTs a `pending` row (via the worker crate's
-- `enqueue_etl_job` helper) and the worker claims it at-most-once, runs the
-- resumable migration, and records terminal state.
--
-- Progress/resume state itself lives in `analytics_migration_state`
-- (last_completed_batch_id); this table only tracks the lifecycle of a single
-- enqueued run request.
CREATE TABLE IF NOT EXISTS etl_jobs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id   UUID NOT NULL REFERENCES projects(id),
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'running', 'done', 'failed')),
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ,
    error        TEXT
);

-- Claim path scans pending jobs oldest-first; partial index keeps the
-- claim query (status = 'pending' ORDER BY requested_at) cheap as done/failed
-- rows accumulate.
CREATE INDEX IF NOT EXISTS etl_jobs_pending_idx
    ON etl_jobs (requested_at)
    WHERE status = 'pending';
