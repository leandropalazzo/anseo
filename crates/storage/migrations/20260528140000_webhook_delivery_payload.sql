-- Story 12.4 follow-up — store the event payload on the delivery row.
--
-- Rationale: webhook retries must send byte-identical bodies across
-- attempts so the consumer's HMAC verification + event_id idempotency
-- remain stable. Reconstructing the payload from primary data
-- (prompt_runs, schedule_ticks) at each retry would couple the
-- dispatcher to every event source AND risk drift if the source row is
-- later edited. Locking the payload at first-emission is simpler and
-- correctness-safe.
--
-- Additive ALTER: `payload_jsonb JSONB` defaults to '{}'::jsonb so any
-- pre-migration rows (currently zero, since 12.4 just landed) parse
-- cleanly into the empty-object case.

ALTER TABLE webhook_deliveries
    ADD COLUMN payload_jsonb JSONB NOT NULL DEFAULT '{}'::jsonb;
