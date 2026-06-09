-- Epic 48 / Story 48.4 — Operator entity-admin: allow `manual_override` as a
-- verification method.
--
-- The operator console (48.4) can manually mark an entity `verified` with a
-- recorded reason (override-verify). That action must reflect a method distinct
-- from the self-service `dns_txt` / `email_magic_link` paths so downstream
-- consumers (and the audit trail) can tell an operator override apart from a
-- user-proven verification. NFR8 ranking eligibility keys on `dns_txt` only, so
-- a `manual_override` badge is — like `email_magic_link` — a lower-trust signal
-- that does NOT qualify for ranked placement.
--
-- This widens the `entities.verification_method` CHECK constraint added in
-- 20260605100000_entities_registry.sql. Forward-only (ARCH D-2).

ALTER TABLE entities
    DROP CONSTRAINT IF EXISTS entities_verification_method_check;

ALTER TABLE entities
    ADD CONSTRAINT entities_verification_method_check
    CHECK (verification_method IS NULL OR
           verification_method IN ('dns_txt', 'email_magic_link', 'manual_override'));
