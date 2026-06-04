-- Brand management (DB-authoritative brand config).
--
-- The `projects` row previously stored only `id` + `name`. Brand variants and
-- the competitor set lived solely in `opengeo.yaml`. Making the DB the source
-- of truth for brand config (so the dashboard can edit it) adds the two
-- columns below. Forward-only, additive (NFR-5): existing rows default to an
-- empty variant list and empty competitor set.
--
-- `competitors` is a JSONB array of objects `{ "name": ..., "variants": [...] }`
-- mirroring `opengeo_core::CompetitorConfig`. JSONB (not a child table) keeps
-- the Phase-1 single-project shape simple; a normalized table is a Phase-4
-- multi-project concern.

ALTER TABLE projects
    ADD COLUMN variants    TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN competitors JSONB  NOT NULL DEFAULT '[]'::jsonb;
