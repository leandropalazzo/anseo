-- Story 27.9 — DSAR + right-to-erasure + crypto-shred.
--
-- DSAR requests: track the lifecycle of data-subject access / erasure requests.
-- Audit tombstoning: audit rows are anonymized (not deleted) when erased.
-- Org offboarding: track the offboarding lifecycle for 27.10.

-- DSAR request tracking table.
CREATE TYPE dsar_kind AS ENUM ('access', 'erasure');
CREATE TYPE dsar_state AS ENUM ('pending', 'in_progress', 'completed', 'rejected');

CREATE TABLE dsar_requests (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    kind            dsar_kind   NOT NULL,
    state           dsar_state  NOT NULL DEFAULT 'pending',
    subject_email   TEXT        NOT NULL,
    legal_basis     TEXT        NOT NULL DEFAULT '',
    requested_by    UUID        NULL REFERENCES operators (id) ON DELETE SET NULL,
    completed_at    TIMESTAMPTZ NULL,
    -- For erasure: records which tables were anonymized and the SLA window used.
    erasure_summary JSONB       NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX dsar_requests_org_idx   ON dsar_requests (org_id, created_at DESC);
CREATE INDEX dsar_requests_state_idx ON dsar_requests (state) WHERE state != 'completed';
CREATE INDEX dsar_requests_email_idx ON dsar_requests (subject_email);

ALTER TABLE dsar_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE dsar_requests FORCE ROW LEVEL SECURITY;

CREATE POLICY dsar_requests_select ON dsar_requests
    FOR SELECT USING (org_id = current_setting('app.org', true)::uuid);

CREATE POLICY dsar_requests_insert ON dsar_requests
    FOR INSERT WITH CHECK (org_id = current_setting('app.org', true)::uuid);

CREATE POLICY dsar_requests_update ON dsar_requests
    FOR UPDATE USING (org_id = current_setting('app.org', true)::uuid);

-- Story 27.10 — Org offboarding lifecycle.
CREATE TYPE offboarding_state AS ENUM (
    'export_grace',   -- cancellation confirmed; org can export data
    'pending_shred',  -- export window elapsed; crypto-shred queued
    'shredded',       -- CMK deleted; ciphertext unrecoverable
    'complete'        -- billing teardown confirmed; row archived
);

CREATE TABLE org_offboarding (
    id                  UUID              PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id              UUID              NOT NULL UNIQUE REFERENCES organizations (id) ON DELETE CASCADE,
    state               offboarding_state NOT NULL DEFAULT 'export_grace',
    -- Stripe subscription / customer refs for teardown coupling (FR-78).
    stripe_subscription_id TEXT           NULL,
    stripe_customer_id     TEXT           NULL,
    -- Legal hold: if set, shred step is skipped until lifted.
    legal_hold          BOOLEAN           NOT NULL DEFAULT false,
    -- Timestamps for each stage.
    export_grace_ends_at TIMESTAMPTZ      NOT NULL,
    shred_scheduled_at   TIMESTAMPTZ      NULL,
    shredded_at          TIMESTAMPTZ      NULL,
    completed_at         TIMESTAMPTZ      NULL,
    initiated_by        UUID              NULL REFERENCES operators (id) ON DELETE SET NULL,
    created_at          TIMESTAMPTZ       NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ       NOT NULL DEFAULT now()
);

CREATE INDEX org_offboarding_state_idx ON org_offboarding (state) WHERE state != 'complete';
CREATE INDEX org_offboarding_grace_idx ON org_offboarding (export_grace_ends_at)
    WHERE state = 'export_grace';

ALTER TABLE org_offboarding ENABLE ROW LEVEL SECURITY;
ALTER TABLE org_offboarding FORCE ROW LEVEL SECURITY;

CREATE POLICY org_offboarding_select ON org_offboarding
    FOR SELECT USING (org_id = current_setting('app.org', true)::uuid);

CREATE POLICY org_offboarding_insert ON org_offboarding
    FOR INSERT WITH CHECK (org_id = current_setting('app.org', true)::uuid);

CREATE POLICY org_offboarding_update ON org_offboarding
    FOR UPDATE USING (org_id = current_setting('app.org', true)::uuid);
