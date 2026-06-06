-- Epic 44 / Story 44.2 — Identified contribution pipeline + server-side brand
-- resolution.
--
-- The OSS client (44.1) transmits an identified contribution carrying a
-- verification_token (43.2) and NEVER a raw brand name. This migration adds the
-- SERVER side: the registry-FK linkage that records which verified brand a
-- contribution resolved to, plus an append-only audit ledger of every
-- token→brand resolution decision.
--
-- Design:
--   * `contributions.entity_domain` — nullable FK into the entity registry
--     (`entities.domain`, the registry PK). This is the ONLY place a
--     contribution is associated with a brand: linkage is via the registry FK,
--     the raw domain is NOT stored as a free-text body field (AC-1). NULL is
--     impossible in practice for rows the ingest path writes (the path refuses
--     to persist unless the token resolved to a currently-verified domain,
--     AC-3), but the column is nullable so the FK can be added to the existing
--     44.1 table without a backfill, and ON DELETE RESTRICT preserves the
--     linkage for the named-leaderboard query (44.3).
--   * `contribution_resolutions` — append-only audit ledger (CC-NFR2 / AC "audit
--     every resolution"). Records every resolution ATTEMPT — accepted and
--     refused alike — with the decision, the reason, and the token hash (never
--     the raw token). A refused attempt has no contribution row, so the audit
--     row's `contribution_id` is nullable.
--
-- Forward-only (ARCH D-2). The `contributions` table already exists (44.1's
-- 20260606120000 migration); we ALTER it rather than re-create.

-- 1. Registry-FK linkage to the resolved verified brand. ON DELETE RESTRICT so a
--    referenced entity cannot be removed out from under live contributions; the
--    erasure path is KEK crypto-shred (39.2), not row deletion.
ALTER TABLE contributions
    ADD COLUMN IF NOT EXISTS entity_domain TEXT NULL
        REFERENCES entities(domain) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_contributions_entity
    ON contributions (entity_domain)
    WHERE entity_domain IS NOT NULL;

-- 2. Append-only audit ledger for server-side brand resolution (CC-NFR2). Every
--    decision is recorded: 'resolved' (stored), 'unverified' (claim_status !=
--    verified → 403), 'unknown_token' (token resolved to no domain → 403),
--    'seal_rejected' (KEK open/AEAD failure → 403), 'kek_missing' (no KEK → 403).
CREATE TABLE IF NOT EXISTS contribution_resolutions (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id       UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    -- SHA-256 of the presented verification token; the raw token is never stored
    -- (mirrors verification_attempts.token_hash posture).
    token_hash       TEXT NOT NULL,
    -- The domain the token resolved to, when it resolved at all (NULL otherwise).
    resolved_domain  TEXT NULL,
    -- claim_status observed at resolution time (audit snapshot), NULL if unknown.
    claim_status     TEXT NULL,
    decision         TEXT NOT NULL
        CHECK (decision IN (
            'resolved', 'unverified', 'unknown_token', 'seal_rejected', 'kek_missing'
        )),
    reason           TEXT NULL,
    -- The stored contribution, when one was written (decision = 'resolved').
    contribution_id  UUID NULL REFERENCES contributions(id) ON DELETE SET NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_contribution_resolutions_project
    ON contribution_resolutions (project_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_contribution_resolutions_token
    ON contribution_resolutions (token_hash);
