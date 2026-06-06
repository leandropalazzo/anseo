-- Epic 47 / Story 47.4 — Operator analytics dashboard.
--
-- The 47.1 `site_event_rollups` table aggregates by `(event_type, day)` only;
-- it deliberately drops the per-row `path` and `referrer` dimensions. The
-- operator Site Overview panel needs top-pages and top-referrers, which are
-- aggregate (GROUP BY) views with no PII — `path` is a site-relative path and
-- `referrer` is a bare domain by the 47.1 privacy contract (never a full URL,
-- never an IP, never an identity). To serve them from a durable aggregate that
-- survives the 30-day raw-retention prune (rather than re-scanning raw rows),
-- we roll them up into two narrow dimension tables alongside the event rollup.
--
-- Forward-only; `IF NOT EXISTS` so a re-apply (or a parallel agent that already
-- created the table) is a no-op rather than a 42P07 collision. ALTER-not-CREATE
-- discipline does not apply here — these are net-new tables, not changes to an
-- existing one.
--
-- PRIVACY: same contract as 47.1. No IP, no session_id, no identity. `path` and
-- `referrer` carry no personal data; these are pure traffic aggregates.

-- ── Top pages (page_view path counts by day) ─────────────────────────────────
CREATE TABLE IF NOT EXISTS site_page_rollups (
    path        TEXT NOT NULL,
    day         DATE NOT NULL,
    views       BIGINT NOT NULL DEFAULT 0,
    finalized   BOOLEAN NOT NULL DEFAULT FALSE,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (path, day)
);

CREATE INDEX IF NOT EXISTS site_page_rollups_day ON site_page_rollups (day DESC);

-- ── Top referrers (referrer-domain visit counts by day) ──────────────────────
CREATE TABLE IF NOT EXISTS site_referrer_rollups (
    referrer    TEXT NOT NULL,           -- bare domain only (47.1 contract)
    day         DATE NOT NULL,
    visits      BIGINT NOT NULL DEFAULT 0,
    finalized   BOOLEAN NOT NULL DEFAULT FALSE,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (referrer, day)
);

CREATE INDEX IF NOT EXISTS site_referrer_rollups_day ON site_referrer_rollups (day DESC);
