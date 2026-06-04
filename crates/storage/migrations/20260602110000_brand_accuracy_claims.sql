-- Epic 34 / FR-R8 brand-accuracy substrate.
--
-- Open-core split: extracted claims and brand ground-truth facts are OSS data.
-- Premium hallucination evaluation reads these rows later but does not own
-- their persistence model.

CREATE TABLE extracted_claims (
    id              UUID PRIMARY KEY,
    prompt_run_id   UUID NOT NULL REFERENCES prompt_runs(id) ON DELETE CASCADE,
    entity          TEXT NOT NULL,
    claim_text      TEXT NOT NULL,
    claim_kind      TEXT NOT NULL DEFAULT 'factual_statement'
        CHECK (claim_kind IN ('factual_statement')),
    char_offset     INT NULL,
    confidence      SMALLINT NOT NULL DEFAULT 100
        CHECK (confidence BETWEEN 0 AND 100),
    extractor_lane  TEXT NOT NULL DEFAULT 'deterministic_sentence'
        CHECK (extractor_lane IN ('deterministic_sentence', 'non_deterministic')),
    organization_id UUID NULL,
    tenant_id       UUID NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE brand_ground_truth_facts (
    id              UUID PRIMARY KEY,
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    entity          TEXT NOT NULL,
    fact_key        TEXT NOT NULL,
    fact_value      TEXT NOT NULL,
    source_url      TEXT NULL,
    source_label    TEXT NULL,
    source_type     TEXT NULL,
    valid_from      TIMESTAMPTZ NULL,
    valid_to        TIMESTAMPTZ NULL,
    organization_id UUID NULL,
    tenant_id       UUID NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT brand_ground_truth_valid_window_check
        CHECK (valid_to IS NULL OR valid_from IS NULL OR valid_to > valid_from),
    CONSTRAINT brand_ground_truth_fact_unique
        UNIQUE (project_id, entity, fact_key)
);

CREATE INDEX idx_extracted_claims_prompt_run_id
    ON extracted_claims (prompt_run_id);
CREATE INDEX idx_extracted_claims_entity
    ON extracted_claims (entity);
CREATE INDEX idx_brand_ground_truth_facts_project_entity
    ON brand_ground_truth_facts (project_id, entity);
