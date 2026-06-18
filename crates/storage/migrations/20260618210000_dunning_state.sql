-- Story 24.4 — dunning state machine for payment failure lifecycle.
--
-- States (D-P4-4):
--   active        — subscription current; full access.
--   grace         — payment failed; 7-day read-only window begins.
--   suspended     — grace expired; schedules paused, data retained.
--   pending_delete — 30 days since grace started; purge queued.
--
-- Transitions are driven by a nightly dunning worker job that reads
-- grace_started_at and advances the state when deadlines elapse.

CREATE TYPE dunning_state AS ENUM ('active', 'grace', 'suspended', 'pending_delete');

ALTER TABLE org_entitlements
    ADD COLUMN dunning_state   dunning_state  NOT NULL DEFAULT 'active',
    ADD COLUMN grace_started_at TIMESTAMPTZ   NULL;

CREATE INDEX org_entitlements_dunning_idx
    ON org_entitlements (dunning_state)
    WHERE dunning_state != 'active';
