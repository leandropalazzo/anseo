-- Story 34.2: accuracy verdict store for hallucination evaluation results.
CREATE TABLE accuracy_verdicts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    claim_id        UUID        NOT NULL,   -- FK to extracted_claims
    entity          TEXT        NOT NULL,
    status          TEXT        NOT NULL CHECK (status IN ('accurate','inaccurate','unverifiable','premium_disabled')),
    rationale       TEXT        NOT NULL,
    matched_fact    TEXT        NULL,
    provider        TEXT        NULL,       -- which LLM provider's response contained the claim
    severity        TEXT        NOT NULL DEFAULT 'medium' CHECK (severity IN ('low','medium','high','critical')),
    org_id          UUID        NULL,
    project_id      UUID        NULL,
    evaluated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX accuracy_verdicts_org_idx ON accuracy_verdicts (org_id, evaluated_at DESC) WHERE org_id IS NOT NULL;
CREATE INDEX accuracy_verdicts_project_idx ON accuracy_verdicts (project_id, evaluated_at DESC) WHERE project_id IS NOT NULL;

ALTER TABLE accuracy_verdicts ENABLE ROW LEVEL SECURITY;
ALTER TABLE accuracy_verdicts FORCE ROW LEVEL SECURITY;

CREATE POLICY accuracy_verdicts_select ON accuracy_verdicts
    FOR SELECT USING (org_id = current_setting('app.org', true)::uuid);
CREATE POLICY accuracy_verdicts_insert ON accuracy_verdicts
    FOR INSERT WITH CHECK (org_id = current_setting('app.org', true)::uuid);
