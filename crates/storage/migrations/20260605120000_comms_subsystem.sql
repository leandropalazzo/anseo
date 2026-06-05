-- Epic 43 / Story 43.7 — Communications subsystem (transactional + marketing).
--
-- This is a NET-NEW subsystem, completely separate from the operator-alert
-- SMTP in crates/scheduler. It enforces the legal transactional/marketing
-- split (CAN-SPAM / ePrivacy / GDPR Art.21(2)):
--
--   * comms_subscriptions  — per-recipient marketing preference + EU consent.
--   * comms_suppressions   — hard suppression list (unsubscribe, bounce,
--                            complaint). Consulted before EVERY marketing send.
--   * comms_preference_tokens — opaque tokens that grant no-login access to the
--                            preference center / one-click unsubscribe.
--   * comms_send_log       — append-only audit of every send attempt. Recipient
--                            is stored HASHED (sha256), never in cleartext, so
--                            the log is GDPR-minimised.
--
-- Recipients are keyed by `recipient_hash` (sha256 lowercased-trimmed email)
-- everywhere except the subscription row, which retains the cleartext address
-- because it is the operational record the recipient manages via the
-- preference center. Suppression + audit are hash-only.

-- ---------------------------------------------------------------------------
-- comms_subscriptions: the recipient's marketing preference center state.
-- ---------------------------------------------------------------------------
CREATE TABLE comms_subscriptions (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recipient_hash          TEXT NOT NULL UNIQUE,        -- sha256(normalized email)
    email                   TEXT NOT NULL,               -- operational record
    -- Granular marketing toggles (AC-3). `all_marketing_off` is the master
    -- kill-switch; when true, NO marketing of any kind is dispatched.
    rank_change_enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    digest_frequency        TEXT NOT NULL DEFAULT 'weekly'
        CHECK (digest_frequency IN ('off', 'daily', 'weekly', 'monthly')),
    all_marketing_off       BOOLEAN NOT NULL DEFAULT FALSE,
    -- EU consent gate (AC-4). Marketing to an EU-resident recipient requires
    -- explicit opt-in: marketing_consent = TRUE. Soft-opt-in does NOT apply.
    marketing_consent       BOOLEAN NOT NULL DEFAULT FALSE,
    is_eu_resident          BOOLEAN NOT NULL DEFAULT FALSE,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- comms_suppressions: the hard suppression list. Hash-only.
-- ---------------------------------------------------------------------------
CREATE TABLE comms_suppressions (
    recipient_hash          TEXT PRIMARY KEY,            -- sha256(normalized email)
    reason                  TEXT NOT NULL
        CHECK (reason IN ('unsubscribe', 'bounce', 'complaint', 'gdpr_objection')),
    -- 'all' suppresses everything; 'marketing' suppresses marketing only
    -- (transactional verification mail is single-purpose and always allowed).
    scope                   TEXT NOT NULL DEFAULT 'marketing'
        CHECK (scope IN ('marketing', 'all')),
    suppressed_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- comms_preference_tokens: opaque no-login access tokens.
-- ---------------------------------------------------------------------------
-- The token grants access to the preference center / one-click unsubscribe
-- for exactly one recipient. We store the HASH of the token, never the raw
-- value (same posture as verification magic links). The raw token lives only
-- in the email link.
CREATE TABLE comms_preference_tokens (
    token_hash              TEXT PRIMARY KEY,            -- sha256(raw token)
    recipient_hash          TEXT NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Preference tokens are long-lived (CAN-SPAM requires the unsubscribe link
    -- to work for at least 30 days post-send). NULL = never expires.
    expires_at              TIMESTAMPTZ NULL
);

CREATE INDEX idx_comms_preference_tokens_recipient
    ON comms_preference_tokens (recipient_hash);

-- ---------------------------------------------------------------------------
-- comms_send_log: append-only audit of every send attempt (AC-5).
-- ---------------------------------------------------------------------------
CREATE TABLE comms_send_log (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recipient_hash          TEXT NOT NULL,               -- hashed, never cleartext
    -- 'transactional' | 'marketing'
    stream                  TEXT NOT NULL
        CHECK (stream IN ('transactional', 'marketing')),
    -- e.g. 'domain_verification', 'verification_revoked', 'rank_change'
    email_type              TEXT NOT NULL,
    -- 'sent' | 'failed' | 'suppressed' | 'consent_blocked'
    outcome                 TEXT NOT NULL
        CHECK (outcome IN ('sent', 'failed', 'suppressed', 'consent_blocked')),
    error                   TEXT NULL,                   -- populated on failure
    -- Idempotency / retry guard for magic-link mail (AC-5): a magic-link email
    -- is not retried more than once within the expiry window. This dedup key
    -- is set to the token_hash for magic-link sends; NULL otherwise.
    dedup_key               TEXT NULL,
    sent_at                 TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_comms_send_log_recipient
    ON comms_send_log (recipient_hash, sent_at DESC);

-- One successful magic-link send per dedup_key (AC-5: no double-send within
-- the token's validity window). Partial unique index so only 'sent' outcomes
-- with a dedup_key are constrained.
CREATE UNIQUE INDEX uq_comms_send_log_dedup
    ON comms_send_log (dedup_key)
    WHERE dedup_key IS NOT NULL AND outcome = 'sent';
