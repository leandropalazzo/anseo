-- Story 21.3 — MFA (TOTP) enrollment + org MFA-required policy
--
-- mfa_required on organizations: Owner-settable; enforced server-side.
-- totp_enrollments: one active enrollment per operator; secret stored encrypted-at-rest.

ALTER TABLE organizations
    ADD COLUMN mfa_required BOOLEAN NOT NULL DEFAULT false;

-- ---------------------------------------------------------------------------
-- totp_enrollments
-- ---------------------------------------------------------------------------
-- One row per (operator, confirmed) enrollment.
-- `confirmed_at` NULL = pending (QR shown, not yet challenged-successfully).
-- `revoked_at`   non-NULL = disabled (operator or Owner-triggered).
-- Only one row with confirmed_at IS NOT NULL AND revoked_at IS NULL may exist
-- per operator (enforced by partial unique index).

CREATE TABLE totp_enrollments (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    operator_id     UUID        NOT NULL,
    -- TOTP secret encoded with the application-level encryption key (AES-GCM).
    -- Stored as base64(nonce || ciphertext).
    secret_enc      TEXT        NOT NULL,
    -- Set after the first successful TOTP challenge confirms the enrollment.
    confirmed_at    TIMESTAMPTZ NULL,
    -- Set when the enrollment is revoked (operator self-service or Owner action).
    revoked_at      TIMESTAMPTZ NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT totp_enrollments_operator_fk FOREIGN KEY (operator_id)
        REFERENCES operators (id) ON DELETE CASCADE
);

-- Only one active confirmed enrollment per operator.
CREATE UNIQUE INDEX totp_enrollments_active_unique
    ON totp_enrollments (operator_id)
    WHERE confirmed_at IS NOT NULL AND revoked_at IS NULL;

CREATE INDEX totp_enrollments_operator_idx ON totp_enrollments (operator_id);

-- ---------------------------------------------------------------------------
-- mfa_challenges  — time-windowed challenge tokens (rate-limit surface)
-- ---------------------------------------------------------------------------
-- Optional table; used to throttle TOTP attempts (max 5 per 30-second window).
-- Cleaned up by a periodic job or on login success.

CREATE TABLE mfa_challenges (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    operator_id     UUID        NOT NULL REFERENCES operators (id) ON DELETE CASCADE,
    attempt_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    succeeded       BOOLEAN     NOT NULL DEFAULT false
);

CREATE INDEX mfa_challenges_operator_attempt_idx
    ON mfa_challenges (operator_id, attempt_at);
