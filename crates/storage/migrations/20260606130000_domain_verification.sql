-- Epic 43 / Story 43.2 — Domain-ownership verification service.
--
-- Verification is a STATE MACHINE, not an event. This table is the
-- append-only ledger of every verification attempt for dispute evidence
-- (NFR8) and the authorization-attestation record. Tokens are stored HASHED
-- (sha256), never in cleartext — the raw token lives only in the DNS TXT
-- record the operator publishes or the magic-link URL we email.
--
-- Two methods:
--   * dns_txt          — PRIMARY / higher-trust. The claimant publishes
--                        `_anseo-challenge.<domain> IN TXT "anseo-verify=<token>"`
--                        and we resolve + constant-time compare. DNS-TXT is the
--                        only method that qualifies for ranked-leaderboard
--                        badges (NFR8).
--   * email_magic_link — alternate / low-friction. A 30-minute single-use link
--                        to a role address. Lower trust; does NOT qualify for
--                        ranked placement.
--
-- Replay protection: a token row is single-use. `consumed_at` is stamped the
-- first time the token verifies; a second use (or any use after `expires_at`)
-- is rejected (→ 401). The partial unique index guarantees at most one
-- live (unconsumed, unexpired) challenge per (domain, method) so we don't
-- accumulate parallel tokens.
--
-- Rate-limit (CC-NFR4): >5 attempts per domain per rolling hour returns 429.
-- The application counts rows in this table within the window before issuing a
-- new token.
--
-- Forward-only (ARCH D-2). Timestamp-prefixed AFTER the existing 43.x
-- migrations (entities/comms/disputes).

CREATE TABLE verification_attempts (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain              TEXT NOT NULL,                 -- normalized domain
    method              TEXT NOT NULL
        CHECK (method IN ('dns_txt', 'email_magic_link')),
    -- sha256(raw token). Raw token never persisted.
    token_hash          TEXT NOT NULL,
    -- Binds the challenge to the initiating session/claimant (AC-1).
    claimant_session    TEXT NULL,
    -- Role address the magic link was sent to (email method only).
    email_address       TEXT NULL,
    -- Lifecycle state of THIS attempt row (append-only ledger; the entity's
    -- claim_status lives in `entities`). 'revoked' rows are written by the
    -- daily re-verify job when a previously-verified TXT record disappears.
    state               TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'verified', 'failed', 'expired', 'revoked')),
    -- Authorization attestation (AC-7): the version of the attestation text the
    -- claimant agreed to, and when. NOT NULL enforces the claim cannot proceed
    -- without it — the application rejects a missing attestation before INSERT.
    attestation_version TEXT NOT NULL,
    attested_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- High-entropy single-use token expiry: 48h for dns_txt, 30min for email.
    expires_at          TIMESTAMPTZ NOT NULL,
    -- Stamped on first successful verify; presence => token is spent (replay
    -- rejected thereafter).
    consumed_at         TIMESTAMPTZ NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Token lookup on verify (constant-time compare is in the app; this index just
-- locates the candidate row).
CREATE INDEX idx_verification_attempts_token ON verification_attempts (token_hash);

-- Rate-limit window scan (AC-6: count attempts per domain in the last hour).
CREATE INDEX idx_verification_attempts_domain_created
    ON verification_attempts (domain, created_at DESC);

-- At most one live (unconsumed, pending) challenge per (domain, method).
-- A new challenge for the same (domain, method) requires the prior one to be
-- consumed/failed/expired first (the app expires the old row before issuing).
CREATE UNIQUE INDEX uq_verification_live_challenge
    ON verification_attempts (domain, method)
    WHERE state = 'pending' AND consumed_at IS NULL;
