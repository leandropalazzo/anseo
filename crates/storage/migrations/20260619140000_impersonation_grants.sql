-- Story 27.6 — Governed admin impersonation grants.
--
-- An impersonation grant allows a support operator to act within a target org
-- for a bounded time window, with full audit attribution.
--
-- Security invariants:
--   - No BYPASSRLS or superuser path; RLS is enforced via app.org GUC as usual.
--   - Grants expire automatically via expires_at; revocation sets revoked_at.
--   - The real support operator_id is always preserved in audit_events.

CREATE TABLE impersonation_grants (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    support_operator_id UUID        NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    target_org_id       UUID        NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    granted_by          UUID        NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    -- Hard ceiling: grants may not exceed 4 hours.
    expires_at          TIMESTAMPTZ NOT NULL,
    revoked_at          TIMESTAMPTZ NULL,
    -- Reason captured for audit trail.
    reason              TEXT        NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX impersonation_grants_support_idx ON impersonation_grants (support_operator_id, expires_at);
CREATE INDEX impersonation_grants_org_idx     ON impersonation_grants (target_org_id, created_at DESC);
