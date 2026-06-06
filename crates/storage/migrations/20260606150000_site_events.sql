-- Epic 47 / Story 47.1 — Public site-event ingest (privacy-safe analytics).
--
-- Backs `POST /v1/site-events` (unauthenticated public ingest) plus a nightly
-- rollup + 30-day raw retention job. Forward-only; `IF NOT EXISTS` so a
-- re-apply (or a parallel agent that already created the table) is a no-op
-- rather than a 42P07 collision.
--
-- PRIVACY CONTRACT (architecture A2):
--   * NO IP address column — rate-limiting uses the request IP only at the
--     Axum middleware edge; it is never persisted.
--   * NO user IDs, no fingerprint, no cross-session device graph.
--   * `session_id` is an ephemeral random UUID generated client-side per browser
--     session. It exists only to dedupe events within a single visit and is not
--     linked to any identity.
--   * `referrer` stores a referrer DOMAIN only, never a full URL.
-- GDPR/CCPA: no personal data is stored, so no consent banner is required.

-- ── Raw events ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS site_events (
    id          BIGSERIAL PRIMARY KEY,
    event_type  TEXT NOT NULL,
    session_id  UUID NOT NULL,            -- ephemeral, not linked to identity
    path        TEXT,
    referrer    TEXT,                     -- domain only, not full URL
    properties  JSONB NOT NULL DEFAULT '{}'::jsonb,
    ts          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS site_events_type_ts ON site_events (event_type, ts DESC);
CREATE INDEX IF NOT EXISTS site_events_ts      ON site_events (ts DESC);

-- ── Nightly aggregate rollups ────────────────────────────────────────────────
-- One row per (event_type, day). `event_count` is the total events seen;
-- `unique_sessions` is the distinct ephemeral session_id count for that bucket.
-- The rollup is the only surface exposed after raw rows are pruned (A2:
-- aggregate-only after retention).
CREATE TABLE IF NOT EXISTS site_event_rollups (
    event_type      TEXT NOT NULL,
    day             DATE NOT NULL,
    event_count     BIGINT NOT NULL DEFAULT 0,
    unique_sessions BIGINT NOT NULL DEFAULT 0,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (event_type, day)
);

CREATE INDEX IF NOT EXISTS site_event_rollups_day ON site_event_rollups (day DESC);
