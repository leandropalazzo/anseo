-- Story 25.3 — portal flag on brand_grants.
--
-- A portal operator is a Viewer with exactly one brand_grant where is_portal=true.
-- The partial unique constraint enforces the single-brand invariant DB-side.
-- Revocation deletes the row; access check uses has_brand_grant (no new authZ path).

ALTER TABLE brand_grants
    ADD COLUMN is_portal BOOLEAN NOT NULL DEFAULT false;

-- Enforce: at most one portal grant per operator.
CREATE UNIQUE INDEX brand_grants_portal_unique
    ON brand_grants (operator_id)
    WHERE is_portal = true;
