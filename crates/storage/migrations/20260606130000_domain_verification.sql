-- Epic 43 / Story 43.2 — Domain-ownership verification service.
--
-- Verification is a STATE MACHINE, not an event. The append-only ledger of every
-- verification attempt (dispute evidence, NFR8) is `verification_attempts`,
-- which is OWNED by Story 43.3's migration
-- (20260605110000_entity_dedup_tables.sql) — it was forward-declared there so
-- 43.3's conflict logic could reference it. This migration ADAPTS that existing
-- table to what 43.2's flow needs rather than re-creating it (the table already
-- exists by the time this runs; a CREATE would collide → 42P07).
--
-- Reconciliation (43.3 columns are authoritative):
--   * `status`        — lifecycle state of the attempt (43.2 originally called
--                       this `state`). The verify/revoke code uses `status`.
--   * `used_at`       — stamped on first successful verify (43.2 originally
--                       called this `consumed_at`); presence ⇒ token spent.
--   * `claimant_email`— role address the magic link was sent to (43.2 originally
--                       called this `email_address`). Reused as-is.
--   * `attestation_version` / `attested_at` already present from 43.3.
--
-- 43.2 additions to the existing table:
--   * `claimant_session` — binds the challenge to the initiating session/claimant
--     (AC-1), distinct from the email role address.
--   * `'revoked'` added to the `status` CHECK — the daily re-verify job (AC-5)
--     writes a `revoked` ledger row when a previously-verified TXT record
--     disappears.
--   * `uq_verification_live_challenge` — at most one live (pending, unconsumed)
--     challenge per (domain, method) so we don't accumulate parallel tokens.
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
-- Rate-limit (CC-NFR4): >5 attempts per domain per rolling hour returns 429.
--
-- Forward-only (ARCH D-2). Timestamp-prefixed AFTER the 43.3 migration that
-- creates the base table.

-- Session binding (AC-1), distinct from the email role address (claimant_email).
ALTER TABLE verification_attempts
    ADD COLUMN IF NOT EXISTS claimant_session TEXT NULL;

-- Allow the daily re-verify job (AC-5) to record a 'revoked' ledger row when a
-- previously-verified TXT record disappears. The 43.3 base table named this
-- constraint `verification_attempts_status_check` (Postgres default for the
-- inline CHECK on `status`); replace it with the superset that adds 'revoked'.
ALTER TABLE verification_attempts
    DROP CONSTRAINT verification_attempts_status_check,
    ADD CONSTRAINT verification_attempts_status_check
        CHECK (status IN (
            'pending', 'verified', 'failed', 'expired', 'replayed', 'revoked'
        ));

-- At most one live (pending, unconsumed) challenge per (domain, method). A new
-- challenge for the same (domain, method) requires the prior one to be
-- consumed/failed/expired first (the app expires the old row before issuing).
CREATE UNIQUE INDEX IF NOT EXISTS uq_verification_live_challenge
    ON verification_attempts (domain, method)
    WHERE status = 'pending' AND used_at IS NULL;
