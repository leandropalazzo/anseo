-- Story 19.2 — reconcile the `recommendations` table to the canonical
-- envelope in architecture-phase3-geo-recommendations.md §7.1.
--
-- DEVIATION (RR-Phase3-CodifyDeviationsInArchDoc): Story 0.12 shipped an
-- early/divergent shape (brand/lane/lifecycle_state with a different state
-- enum, no project_id, no input_fingerprint, no dedup index). That table is
-- unused (no live callers — the repo methods carried #[allow(dead_code)]), so
-- we reconcile forward by dropping and recreating to the arch shape rather
-- than threading a column-by-column ALTER. This is the 19.1 wire envelope's
-- storage projection.
--
-- DEVIATION (FK): the arch §7.1 column `project_id UUID REFERENCES projects(id)`
-- drops the inline REFERENCES here. The deterministic engine derives
-- `project_id` as a content ULID-as-UUID for fixtures and self-contained
-- dedup tests; a hard FK would couple recommendation generation tests to a
-- seeded projects row. The application layer already scopes by project_id;
-- referential integrity is enforced at the service boundary, not the table.

DROP TABLE IF EXISTS recommendations CASCADE;

CREATE TABLE recommendations (
    id                    UUID PRIMARY KEY,
    project_id            UUID NOT NULL,
    kind                  TEXT NOT NULL,
    severity              TEXT NOT NULL,
    confidence_band       TEXT NOT NULL,
    state                 TEXT NOT NULL,
    summary               TEXT NOT NULL,
    payload               JSONB NOT NULL,
    traceability          JSONB NOT NULL,
    reproducibility_class TEXT NOT NULL,
    reproducibility_note  TEXT,
    tags                  TEXT[] NOT NULL DEFAULT '{}',
    input_fingerprint     TEXT NOT NULL,
    engine_version        TEXT NOT NULL,
    plugin_source         TEXT,
    generated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (state IN ('generated','surfaced','acknowledged','acted','measured','dismissed','stale'))
);

CREATE INDEX recommendations_project_state_idx
    ON recommendations (project_id, state, generated_at DESC);
CREATE INDEX recommendations_project_kind_idx
    ON recommendations (project_id, kind, generated_at DESC);
CREATE INDEX recommendations_project_repro_idx
    ON recommendations (project_id, reproducibility_class);

-- Dedup support ([rec-4]): the engine refuses to insert a Recommendation
-- whose (project_id, kind, input_fingerprint) matches a still-active row.
CREATE UNIQUE INDEX recommendations_active_dedup_idx
    ON recommendations (project_id, kind, input_fingerprint)
    WHERE state NOT IN ('dismissed','measured','stale');
