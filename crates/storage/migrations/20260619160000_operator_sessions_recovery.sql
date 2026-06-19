-- Story 27.11 — Operator profile security: sessions + MFA recovery codes.
--
-- operator_sessions: tracks active login sessions for each operator.
-- mfa_recovery_codes: one-time recovery codes for TOTP re-enrollment after
-- device loss. Stored hashed (SHA-256).

-- Active login sessions per operator.
CREATE TABLE operator_sessions (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    operator_id     UUID        NOT NULL REFERENCES operators (id) ON DELETE CASCADE,
    -- Opaque session token stored as SHA-256 hex (never stored plaintext).
    token_hash      TEXT        NOT NULL UNIQUE,
    user_agent      TEXT        NULL,
    ip_address      INET        NULL,
    last_active_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX operator_sessions_operator_idx
    ON operator_sessions (operator_id, expires_at DESC);
CREATE INDEX operator_sessions_active_idx
    ON operator_sessions (operator_id)
    WHERE revoked_at IS NULL;

-- MFA recovery codes (one-time use, SHA-256 hashed at rest).
-- 10 codes generated at TOTP enrollment; each is consumed exactly once.
CREATE TABLE mfa_recovery_codes (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    operator_id     UUID        NOT NULL REFERENCES operators (id) ON DELETE CASCADE,
    enrollment_id   UUID        NOT NULL REFERENCES totp_enrollments (id) ON DELETE CASCADE,
    code_hash       TEXT        NOT NULL,
    used_at         TIMESTAMPTZ NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX mfa_recovery_codes_operator_idx
    ON mfa_recovery_codes (operator_id)
    WHERE used_at IS NULL;
