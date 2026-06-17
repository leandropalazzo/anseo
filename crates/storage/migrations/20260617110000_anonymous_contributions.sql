-- Epic 40 / Story 40.4 — durable anonymous contribution outbox for ingest.
--
-- `POST /v1/ingest/run` already redacts + envelope-seals the benchmark payload
-- under the project's KEK when `contribute=true` and consent is active. This
-- table stores that SEALED payload durably, linked to the originating
-- prompt_run and the consent row that authorized it. No cleartext benchmark
-- payload is stored here.

CREATE TABLE anonymous_contributions (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    prompt_run_id     UUID NOT NULL REFERENCES prompt_runs(id) ON DELETE CASCADE,
    project_id        UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    project_hmac      TEXT NOT NULL,
    consent_record_id UUID NOT NULL REFERENCES benchmark_consent(id) ON DELETE RESTRICT,
    terms_version     TEXT NOT NULL,
    sealed_payload    JSONB NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (prompt_run_id)
);

CREATE INDEX idx_anonymous_contributions_project
    ON anonymous_contributions (project_id, created_at DESC);

CREATE INDEX idx_anonymous_contributions_consent
    ON anonymous_contributions (consent_record_id);
