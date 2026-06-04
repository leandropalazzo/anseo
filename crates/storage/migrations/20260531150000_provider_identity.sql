-- Provider identity for analytics grouping.
--
-- All OpenRouter runs persist with provider = 'openrouter', but a single
-- OpenRouter key can fan out to many upstream `<vendor>/<model>` models
-- (provider_model_version holds the upstream, e.g. `openai/gpt-4o-...`).
-- For the dashboard's per-provider surfaces we want each upstream to appear
-- as its own identity, while every other provider keeps its plain name.
--
-- A STORED generated column derives this once so every analytics query can
-- GROUP BY / SELECT it consistently instead of repeating the CASE. Both
-- source columns are NOT NULL, and the expression uses only immutable
-- operators, so the generated column is well-defined.

ALTER TABLE prompt_runs
    ADD COLUMN IF NOT EXISTS provider_identity TEXT
    GENERATED ALWAYS AS (
        CASE
            WHEN provider = 'openrouter'
                THEN 'openrouter:' || provider_model_version
            ELSE provider
        END
    ) STORED;

CREATE INDEX IF NOT EXISTS prompt_runs_provider_identity_idx
    ON prompt_runs (provider_identity);
