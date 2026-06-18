-- Story 21.4 — Operator invites lifecycle + session/API-key management
--
-- Changes:
--   1. operators.deactivated_at  — soft deactivation; auth middleware rejects if non-NULL.
--   2. api_keys.minter_operator_id / minter_role — key scope cannot exceed minter's role.

-- ---------------------------------------------------------------------------
-- 1. Operator deactivation
-- ---------------------------------------------------------------------------
ALTER TABLE operators
    ADD COLUMN deactivated_at TIMESTAMPTZ NULL;

-- Partial index: fast lookup of active operators.
CREATE INDEX operators_active_idx
    ON operators (id)
    WHERE deactivated_at IS NULL;

-- ---------------------------------------------------------------------------
-- 2. Per-brand API key scope tracking
-- ---------------------------------------------------------------------------
-- A key is minted by an operator whose role at the time is recorded.
-- The application enforces: key grants ⊆ minter grants (escalation prevention).

ALTER TABLE api_keys
    ADD COLUMN minter_operator_id UUID NULL REFERENCES operators (id) ON DELETE SET NULL,
    ADD COLUMN minter_role        org_role NULL;

COMMENT ON COLUMN api_keys.minter_operator_id IS
    'Operator who created this key; NULL if the minting operator was later deleted.';
COMMENT ON COLUMN api_keys.minter_role IS
    'Role the minter held at key-creation time; key grants must be ⊆ this role''s grants.';
