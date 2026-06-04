-- Site-audit history (Epic 32). Each `ogeo audit` / POST /v1/audit run is
-- persisted so citation-readiness can be tracked over time per project. The
-- full report is kept as JSONB; the scalar columns power the history list.
CREATE TABLE audit_runs (
    id              UUID PRIMARY KEY,
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    target          TEXT NOT NULL,
    overall_score   SMALLINT NOT NULL CHECK (overall_score BETWEEN 0 AND 100),
    pages_crawled   INT NOT NULL,
    gate_passed     BOOLEAN NULL,
    report          JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_runs_project_created
    ON audit_runs (project_id, created_at DESC);
