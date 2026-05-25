-- OpenGEO Phase 1 initial schema migration.
--
-- Decisions captured here (architecture.md / PRD references):
--   D-2 (ARCH-4): Migrations are forward-only. No `*.down.sql` file accompanies
--     this migration; rollbacks happen via a new forward migration that
--     reverses the change. The schema-stability covenant (NFR-5) blocks
--     breaking column removal / rename within a phase — additive ALTERs only.
--   D-5 (ARCH-5): Every Phase 1 tenant-scoped table carries
--     `organization_id UUID NULL` and `tenant_id UUID NULL` even though Phase 1
--     leaves them NULL. Phase 4 backfills, then tightens to NOT NULL.
--   L549: Every table carries `created_at TIMESTAMPTZ NOT NULL DEFAULT now()`.
--     `updated_at` / `deleted_at` are intentionally absent — Phase 1 entities
--     are append-only.
--   `error_kind` is TEXT NULL with a CHECK enumerating the six PRD §11.5 wire
--     strings (NOT a Postgres ENUM). Rationale: NFR-5 forward-compat — adding
--     a future variant (Phase 2 plans `provider_unsupported_model`) requires
--     only a forward-only `ALTER … DROP CONSTRAINT … ADD CONSTRAINT …`, never
--     a `DROP TYPE`. Same pattern applies to `prompt_runs.status`.
--   `citations.source_type` is TEXT NULL with NO CHECK. Rationale: the set
--     ({docs, reddit, wikipedia, youtube, general_web}) is expected to grow as
--     extractors are added; a Phase 2 story may add a CHECK once the set
--     stabilises. Document the asymmetry here, not in code comments.
--   FK ON DELETE policy is explicit on every FK, never relying on Postgres
--     defaults:
--       - prompts.project_id      -> projects(id)     ON DELETE RESTRICT
--       - prompt_runs.prompt_id   -> prompts(id)      ON DELETE RESTRICT
--       - mentions.prompt_run_id  -> prompt_runs(id)  ON DELETE CASCADE
--       - citations.prompt_run_id -> prompt_runs(id)  ON DELETE CASCADE
--     Parent-of-audit refs (project, prompt, prompt_run) RESTRICT to protect
--     audit integrity; derived rows (mentions, citations) CASCADE because they
--     are produced from the parent's raw_response and cannot outlive it.
--   ULID-as-UUID: PKs are `UUID` columns; Rust callers pass ULID newtypes
--     (`opengeo_core::ids::*`) directly via the `sqlx` feature on `core`.

CREATE TABLE projects (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    organization_id UUID NULL,
    tenant_id       UUID NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE prompts (
    id              UUID PRIMARY KEY,
    project_id      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    name            TEXT NOT NULL,
    text            TEXT NOT NULL,
    organization_id UUID NULL,
    tenant_id       UUID NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE prompt_runs (
    id                     UUID PRIMARY KEY,
    prompt_id              UUID NOT NULL REFERENCES prompts(id) ON DELETE RESTRICT,
    provider               TEXT NOT NULL,
    provider_model_version TEXT NOT NULL,
    provider_region        TEXT NULL,
    started_at             TIMESTAMPTZ NOT NULL,
    finished_at            TIMESTAMPTZ NULL,
    raw_response           JSONB NOT NULL DEFAULT '{}'::jsonb,
    request_parameters     JSONB NOT NULL DEFAULT '{}'::jsonb,
    status                 TEXT NOT NULL CHECK (status IN ('ok', 'failed')),
    error_kind             TEXT NULL CHECK (
        error_kind IS NULL OR error_kind IN (
            'provider_unauthorized',
            'provider_rate_limited',
            'provider_timeout',
            'provider_5xx',
            'provider_invalid_response',
            'network_error'
        )
    ),
    organization_id        UUID NULL,
    tenant_id              UUID NULL,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE mentions (
    id              UUID PRIMARY KEY,
    prompt_run_id   UUID NOT NULL REFERENCES prompt_runs(id) ON DELETE CASCADE,
    entity          TEXT NOT NULL,
    char_offset     INT NOT NULL,
    rank            INT NOT NULL,
    matched_text    TEXT NOT NULL,
    organization_id UUID NULL,
    tenant_id       UUID NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE citations (
    id              UUID PRIMARY KEY,
    prompt_run_id   UUID NOT NULL REFERENCES prompt_runs(id) ON DELETE CASCADE,
    url             TEXT NULL,
    domain          TEXT NOT NULL,
    frequency       INT NOT NULL DEFAULT 1,
    source_type     TEXT NULL,
    organization_id UUID NULL,
    tenant_id       UUID NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Indexes per architecture.md L544–547 (`idx_<table>_<columns>`).
CREATE INDEX idx_prompts_project_id              ON prompts (project_id);
CREATE INDEX idx_prompt_runs_prompt_id_started_at ON prompt_runs (prompt_id, started_at);
CREATE INDEX idx_prompt_runs_status              ON prompt_runs (status);
CREATE INDEX idx_mentions_prompt_run_id          ON mentions (prompt_run_id);
CREATE INDEX idx_mentions_entity                 ON mentions (entity);
CREATE INDEX idx_citations_prompt_run_id         ON citations (prompt_run_id);
CREATE INDEX idx_citations_domain                ON citations (domain);
