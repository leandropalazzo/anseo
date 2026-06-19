-- Story 27.5 — Per-brand cost attribution.
--
-- Adds:
--   run_cost_usd_per_run(provider TEXT) → estimated USD per run at the
--     blended provider rate from crates/providers/src/cost.rs (kept in sync).
--   brand_cost_summary VIEW — aggregates prompt_runs by org + brand (project)
--     and computes estimated_cost_usd for rebilling and margin tracking.
--     [p4-cost-1] evidence: per-brand LLM cost queryable by org_id + brand_id.

-- ---------------------------------------------------------------------------
-- Provider blended rate function (matches Rust cost.rs DEFAULT table)
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION run_cost_usd_per_run(provider TEXT)
RETURNS NUMERIC AS $$
    SELECT CASE lower(provider)
        WHEN 'openai'      THEN 2000.0 * 7.50 / 1000000.0
        WHEN 'anthropic'   THEN 2000.0 * 9.00 / 1000000.0
        WHEN 'gemini'      THEN 2000.0 * 5.25 / 1000000.0
        WHEN 'perplexity'  THEN 2000.0 * 5.00 / 1000000.0
        WHEN 'grok'        THEN 2000.0 * 7.00 / 1000000.0
        WHEN 'mistral'     THEN 2000.0 * 4.00 / 1000000.0
        WHEN 'openrouter'  THEN 2000.0 * 7.50 / 1000000.0
        ELSE 0.0
    END::NUMERIC(18, 10)
$$ LANGUAGE sql IMMUTABLE STRICT;

-- ---------------------------------------------------------------------------
-- brand_cost_summary VIEW
-- [p4-cost-1]: per-brand LLM cost queryable by org_id + brand_id.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE VIEW brand_cost_summary AS
SELECT
    pr.org_id,
    p.project_id                              AS brand_id,
    date_trunc('day', pr.created_at)::date    AS cost_date,
    pr.provider,
    count(*)::bigint                          AS run_count,
    (count(*) * run_cost_usd_per_run(pr.provider))::NUMERIC(18, 8) AS estimated_cost_usd
FROM prompt_runs pr
JOIN prompts p ON p.id = pr.prompt_id
WHERE pr.status = 'ok'
  AND pr.org_id IS NOT NULL
GROUP BY pr.org_id, p.project_id, date_trunc('day', pr.created_at)::date, pr.provider;
