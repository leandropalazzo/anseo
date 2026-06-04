-- Prompt tags (free-form labels).
--
-- Prompts gain a set of free-form string tags so operators can group tracked
-- queries (e.g. "comparison", "alternatives") and the dashboard can roll up
-- run metrics by tag. AI-generated prompts carry the literal tag "AUTO" when
-- no existing tag is a better match. Forward-only, additive (NFR-5): existing
-- rows default to an empty tag set.

ALTER TABLE prompts
    ADD COLUMN tags TEXT[] NOT NULL DEFAULT '{}';
