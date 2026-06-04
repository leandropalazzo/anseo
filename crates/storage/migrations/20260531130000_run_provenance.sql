-- run provenance: lifecycle/step audit log for a prompt run (story 31-3).
-- One row per lifecycle stage (provider_call, response_persisted,
-- mention_extraction, citation_extraction, ranking) recorded as a run flows
-- through the orchestrator write path. Read by GET /runs/:id/provenance.
--
-- Forward-only. org/tenant columns mirror the per-run convention on
-- mentions/citations (nullable, single-tenant default in Phase 1).
CREATE TABLE IF NOT EXISTS run_provenance (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    prompt_run_id   UUID NOT NULL REFERENCES prompt_runs(id) ON DELETE CASCADE,
    step            TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('ok', 'error', 'skipped')),
    detail          JSONB NOT NULL DEFAULT '{}'::jsonb,
    at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    organization_id UUID NULL,
    tenant_id       UUID NULL
);

CREATE INDEX idx_run_provenance_prompt_run_id_at
    ON run_provenance (prompt_run_id, at);
