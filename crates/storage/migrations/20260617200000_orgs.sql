-- Story 20.1 — Phase 4 multi-tenant org/operator data model (D-P4-7).
--
-- Forward-only migration (ARCH-4 invariant); no rollback path.
-- Adds the core org substrate:
--   organizations       — top-level tenant; slug + region pin.
--   operators           — human actors (Anseo accounts).
--   operator_org_roles  — join table: one operator may hold one role per org.
--   brand_grants        — per-operator, per-brand access grant (Operator/Viewer
--                         roles require an explicit row; Owner/Admin bypass).
--   org_invites         — lifecycle state machine for email-based onboarding.
--
-- No tenant tables are touched here (that is 20.2 — org_id backfill).
-- Self-host single-tenant data is fully unaffected.

-- --------------------------------------------------------------------------
-- organizations
-- --------------------------------------------------------------------------
CREATE TABLE organizations (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    slug       TEXT        NOT NULL,
    name       TEXT        NOT NULL,
    -- D-P4-5: region pin for data-residency compliance; NULL = default region.
    region     TEXT        NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT organizations_slug_unique UNIQUE (slug),
    CONSTRAINT organizations_slug_format CHECK (slug ~ '^[a-z0-9][a-z0-9\-]{0,61}[a-z0-9]$')
);

CREATE INDEX organizations_slug_idx ON organizations (slug);
CREATE INDEX organizations_created_at_idx ON organizations (created_at);

-- --------------------------------------------------------------------------
-- operators (Anseo accounts — distinct from API key / CLI operator)
-- --------------------------------------------------------------------------
CREATE TABLE operators (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Unique stable identity handle; sourced from the IdP (GitHub login /
    -- email) — never user-changeable without explicit rename flow.
    login          TEXT        NOT NULL,
    display_name   TEXT        NULL,
    email          TEXT        NULL,
    -- IdP subject claim; NULL until first OAuth login.
    idp_sub        TEXT        NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT operators_login_unique UNIQUE (login)
);

CREATE INDEX operators_login_idx     ON operators (login);
CREATE INDEX operators_idp_sub_idx   ON operators (idp_sub) WHERE idp_sub IS NOT NULL;

-- --------------------------------------------------------------------------
-- operator_org_roles  — role matrix per (operator, org)
-- --------------------------------------------------------------------------
-- Roles (D-P4-9 §5.2):
--   owner    — full admin; cannot be demoted except by another owner.
--   admin    — all capabilities except owner management.
--   operator — read + write brand data; requires brand_grants row.
--   viewer   — read-only brand data; requires brand_grants row.
--   billing  — billing endpoints only; no brand-data read (FR-78).
CREATE TYPE org_role AS ENUM ('owner', 'admin', 'operator', 'viewer', 'billing');

CREATE TABLE operator_org_roles (
    operator_id UUID     NOT NULL REFERENCES operators (id) ON DELETE CASCADE,
    org_id      UUID     NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    role        org_role NOT NULL,
    granted_by  UUID     NULL     REFERENCES operators (id),
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (operator_id, org_id)
);

CREATE INDEX operator_org_roles_org_idx ON operator_org_roles (org_id);

-- --------------------------------------------------------------------------
-- brand_grants  — per-operator, per-brand grant for Operator/Viewer roles.
-- Owner/Admin bypass this table (implicit all-brands) per the authZ matrix.
-- --------------------------------------------------------------------------
CREATE TABLE brand_grants (
    operator_id UUID        NOT NULL REFERENCES operators (id) ON DELETE CASCADE,
    -- project_id is the stable brand identifier used throughout Phase 1–3.
    -- D-P4-7: brand_id is an alias; no rename to keep /v1 payloads frozen.
    project_id  UUID        NOT NULL,
    org_id      UUID        NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    granted_by  UUID        NULL     REFERENCES operators (id),
    granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (operator_id, project_id)
);

CREATE INDEX brand_grants_org_idx     ON brand_grants (org_id);
CREATE INDEX brand_grants_project_idx ON brand_grants (project_id);

-- --------------------------------------------------------------------------
-- org_invites  — email-based onboarding state machine.
-- States: pending → invited → accepted | failed | expired
-- --------------------------------------------------------------------------
CREATE TYPE invite_state AS ENUM ('pending', 'invited', 'accepted', 'failed', 'expired');

CREATE TABLE org_invites (
    id              UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID         NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    invited_email   TEXT         NOT NULL,
    role            org_role     NOT NULL DEFAULT 'operator',
    state           invite_state NOT NULL DEFAULT 'pending',
    -- Cryptographically random token; hashed before storage (SHA-256 hex).
    token_hash      TEXT         NOT NULL,
    invited_by      UUID         NULL REFERENCES operators (id),
    invited_at      TIMESTAMPTZ  NULL,
    accepted_at     TIMESTAMPTZ  NULL,
    expires_at      TIMESTAMPTZ  NOT NULL DEFAULT (now() + INTERVAL '7 days'),
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT now(),

    CONSTRAINT org_invites_email_pending_unique
        UNIQUE (org_id, invited_email, state)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX org_invites_org_idx    ON org_invites (org_id);
CREATE INDEX org_invites_email_idx  ON org_invites (invited_email);
CREATE INDEX org_invites_state_idx  ON org_invites (state);
CREATE INDEX org_invites_expires_idx ON org_invites (expires_at) WHERE state IN ('pending', 'invited');

-- Prevent token_hash timing oracle: constant-time comparison is enforced in
-- app code (Story 20.8 /v1/org-invites/accept); the column index is omitted
-- to avoid hinting the planner toward an exploitable sequential comparison.
