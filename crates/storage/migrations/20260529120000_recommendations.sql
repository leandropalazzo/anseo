-- Story 0.12 — Recommendations storage substrate for Epic 17 (GEO
-- Recommendations) and Epic 19 (Plugin SDK). See
-- `_bmad-output/planning-artifacts/architecture-phase3-geo-recommendations.md`
-- §7 for the storage section that motivates this shape.
--
-- A recommendation is a system- or plugin-generated suggestion to the
-- operator: e.g. "this brand is mentioned but not cited in N runs — add
-- structured-data markup". Recommendations move through a lifecycle
-- (`generated → surfaced → snoozed | acted_on → measured`). Outcome
-- columns (`outcome_visibility_delta`, `outcome_window_days`) are filled
-- in by a follow-up window job after `acted_at`, so they're nullable at
-- insert time.
--
-- `kind` is a free-form discriminator. Built-in kinds:
--   - `docs_not_cited`
--   - `competitor_outranks`
--   - `structural_change_to_content`
-- Plugin-emitted kinds MUST be namespaced `plugin:<name>:<kind>` and the
-- emitting plugin's name is stored in `plugin_source` for L6 quarantine
-- (a misbehaving plugin's recommendations can be filtered or hidden
-- without dropping rows).
--
-- `traceability` is JSONB without a CHECK constraint — the schema is
-- documented per-kind in the recommender code and may evolve. Expected
-- top-level keys at the time of writing:
--   {
--     "inputs":        [...],          -- input fact ids / urls
--     "model":         "claude-..." | null,
--     "prompts":       [...] | null,   -- prompt template ids used
--     "source_run_ids":[uuid, ...]     -- prompt_runs that triggered it
--   }
-- Free-form so plugins can extend without a migration.
--
-- `lane` discriminates how the recommendation was produced:
--   - `heuristic`  — deterministic rule over prompt_runs / mentions
--   - `llm_aided`  — LLM read the evidence and wrote the suggestion
--   - `hybrid`     — heuristic candidate, LLM ranked / phrased
-- `non_deterministic_pipeline` is a derived convenience flag (TRUE iff
-- lane != 'heuristic') so the UI can mark these for the operator without
-- joining a config table.

CREATE TABLE recommendations (
    id                          UUID PRIMARY KEY,
    kind                        TEXT NOT NULL,
    prompt                      TEXT NULL,
    provider                    TEXT NULL,
    brand                       TEXT NOT NULL,
    severity                    TEXT NOT NULL CHECK (severity IN ('low', 'medium', 'high')),
    lane                        TEXT NOT NULL CHECK (lane IN ('heuristic', 'llm_aided', 'hybrid')),
    non_deterministic_pipeline  BOOLEAN NOT NULL,
    plugin_source               TEXT NULL,
    generated_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    lifecycle_state             TEXT NOT NULL DEFAULT 'generated'
                                CHECK (lifecycle_state IN ('generated', 'surfaced', 'snoozed', 'acted_on', 'measured')),
    acted_at                    TIMESTAMPTZ NULL,
    evidence_url                TEXT NULL,
    outcome_visibility_delta    REAL NULL,
    outcome_window_days         INTEGER NULL,
    traceability                JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX idx_recommendations_brand_generated_at
    ON recommendations (brand, generated_at DESC);
CREATE INDEX idx_recommendations_lifecycle_state
    ON recommendations (lifecycle_state);
-- Partial index: only plugin-sourced rows are interesting for the
-- quarantine filter; built-in recommendations dominate the table and
-- would bloat a full index for no reader.
CREATE INDEX idx_recommendations_plugin_source
    ON recommendations (plugin_source)
    WHERE plugin_source IS NOT NULL;
