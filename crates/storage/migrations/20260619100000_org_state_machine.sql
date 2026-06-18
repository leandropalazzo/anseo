-- Story 27.1 — Org state machine (FR-87) + signup operator password.
--
-- Org lifecycle states (FR-87):
--   unconfigured  — created but provisioning saga not complete
--   trial         — provisioning done; trial period active (default for new orgs)
--   active        — full billing confirmed
--
-- operators.password_hash — Argon2id hash for email/password auth.
--   NULL for OIDC-only operators (Story 21.2 when available).
--   [mock-OK]: live OIDC wired in Story 21.2; this column supports email/password.
--
-- operators.email_verified_at — NULL until the operator confirms their email.
--   Required before org provisioning completes.

CREATE TYPE org_state AS ENUM ('unconfigured', 'trial', 'active');

ALTER TABLE organizations
    ADD COLUMN state org_state NOT NULL DEFAULT 'trial';

ALTER TABLE operators
    ADD COLUMN password_hash     TEXT        NULL,
    ADD COLUMN email_verified_at TIMESTAMPTZ NULL;

CREATE INDEX operators_email_idx ON operators (email) WHERE email IS NOT NULL;

-- ---------------------------------------------------------------------------
-- org_provisioning_sagas — idempotent, resumable signup saga state.
-- ---------------------------------------------------------------------------
-- Each signup attempt gets one row. Steps advance monotonically; retrying a
-- saga re-enters at the last completed step.
--
-- Steps (saga_step enum):
--   created     — row inserted, operator created, org created (state=unconfigured)
--   kms_done    — per-org KMS CMK provisioned (23.1 KmsOrgStore)
--   entitlement — default Free entitlement upserted (24.1)
--   owner_set   — signer added to operator_org_roles as Owner
--   complete    — email verified; org.state → trial

CREATE TYPE saga_step AS ENUM (
    'created', 'kms_done', 'entitlement', 'owner_set', 'complete'
);

CREATE TABLE org_provisioning_sagas (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Idempotency key: one active saga per operator (email).
    operator_id     UUID        NOT NULL REFERENCES operators (id) ON DELETE CASCADE,
    org_id          UUID        NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    -- Current step — monotonically advancing.
    step            saga_step   NOT NULL DEFAULT 'created',
    -- Cryptographically random email verification token; SHA-256 hex before storage.
    verify_token_hash TEXT      NULL,
    verify_expires_at TIMESTAMPTZ NULL,
    completed_at    TIMESTAMPTZ NULL,
    -- Cleanup hook: if saga is abandoned, the org is left in unconfigured state.
    -- A nightly job deletes orgs in unconfigured state older than 24h.
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- One active saga per operator at a time.
    CONSTRAINT saga_operator_unique UNIQUE (operator_id)
        DEFERRABLE INITIALLY DEFERRED
);

CREATE INDEX sagas_org_idx        ON org_provisioning_sagas (org_id);
CREATE INDEX sagas_step_idx       ON org_provisioning_sagas (step) WHERE step != 'complete';
CREATE INDEX sagas_expires_idx    ON org_provisioning_sagas (verify_expires_at)
    WHERE step != 'complete' AND verify_expires_at IS NOT NULL;
