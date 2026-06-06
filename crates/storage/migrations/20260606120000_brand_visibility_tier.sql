-- Epic 44 / Story 44.1 — Brand-visibility (identified) consent tier on the OSS
-- client.
--
-- The anonymous aggregate tier (Story 13.1) already records consent in
-- `benchmark_consent`. The identified tier is the EXPLICIT, separately-revocable
-- opt-in that permits transmitting brand identity (via a verification token —
-- NOT a raw brand name) on the identified contribution path. APPEARING ≠
-- CLAIMING: a project only carries identity when this tier is active.
--
-- Design:
--   * `tier` distinguishes the anonymous-aggregate consent stream from the
--     brand-visibility stream. The two are recorded and revoked independently,
--     so a project can be anonymously opted in while remaining identified-out
--     (and vice versa). Existing rows predate the tier split and default to
--     'anonymous' (CC-NFR2 append-only — we never rewrite history; the column
--     default backfills semantically-correct provenance).
--   * `contributions` gains a NON-nullable `consent_record_id` FK for
--     identified-tier rows, so every identified contribution is traceable to the
--     consent event that authorized it (CC-NFR2 / readiness gap C3 — the FK that
--     44.2 tests for is created here). Anonymous-tier rows leave it NULL.
--
-- Forward-only (ARCH D-2).

-- 1. Tier the consent stream. Default 'anonymous' so the Story 13.1 rows keep
--    their original meaning; the brand-visibility CLI verb writes 'brand_visibility'.
ALTER TABLE benchmark_consent
    ADD COLUMN tier TEXT NOT NULL DEFAULT 'anonymous'
        CHECK (tier IN ('anonymous', 'brand_visibility'));

-- Most-recent-event-per-(project, tier) lookup: the redactor / status command
-- asks "is the brand_visibility tier currently active for this project?"
-- independently of the anonymous tier.
CREATE INDEX idx_benchmark_consent_project_tier
    ON benchmark_consent (project_id, tier, created_at DESC);

-- 2. The identified-tier contribution ledger. This is the OSS-side table the
--    server-side brand resolution (44.2) and named leaderboard (44.3) build on.
--    `consent_record_id` is NOT NULL: an identified contribution cannot exist
--    without the consent event that authorized it (CC-NFR2 provenance).
--
--    The contribution payload itself is NOT stored here in cleartext — it crosses
--    the redaction boundary as a SealedContribution. This table carries only the
--    linkage HMAC, the identified-tier verification token, and the consent
--    provenance FK.
CREATE TABLE contributions (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id         UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    -- Cleartext linkage identifier (HMAC over the project id); grouping only,
    -- NOT an erasure mechanism (erasure = destroy the KEK, see crates/benchmark).
    project_hmac       TEXT NOT NULL,
    -- Identified-tier consent provenance. NOT NULL: every row in this table is an
    -- identified contribution and MUST point at the consent event that allowed it.
    consent_record_id  UUID NOT NULL REFERENCES benchmark_consent(id) ON DELETE RESTRICT,
    -- The verification token (43.2) that resolves to brand identity server-side.
    -- Identity is carried ONLY via this token — never a raw brand name.
    verification_token TEXT NOT NULL,
    terms_version      TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_contributions_project ON contributions (project_id, created_at DESC);
CREATE INDEX idx_contributions_consent ON contributions (consent_record_id);
