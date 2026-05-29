-- Story 13.1 — public-benchmark consent + audit storage (architecture
-- §2.3 / OQ-7 / D-9). Records every operator opt-in / opt-out event so
-- the redactor can verify pinned terms_version is current AND so the
-- audit trail outlasts the consent itself.
--
-- One project may have many `benchmark_consent` rows over time — the
-- redactor uses the most-recent row's terms_version to decide whether
-- the operator is on the current terms. Opt-out is recorded as a new
-- `event = 'optout'` row, not a delete (architecture: "soft opt-out;
-- historical contributions stay attributable").

CREATE TABLE benchmark_consent (
    id              UUID PRIMARY KEY,
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    event           TEXT NOT NULL CHECK (event IN ('optin', 'optout')),
    terms_version   TEXT NOT NULL,
    actor           TEXT NULL,
    note            TEXT NULL,
    organization_id UUID NULL,
    tenant_id       UUID NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_benchmark_consent_project_id ON benchmark_consent (project_id, created_at DESC);
