-- Project archival for multi-project coexistence (Epic 36 / Story 36.1).
--
-- Until now a deployment held exactly one project (the single-brand pin).
-- Epic 36 lets a single operator run several projects side by side, so the
-- registry needs a soft-delete: archived projects stay in the DB (their data is
-- preserved and FK-referenced by children) but drop out of the active listing.
--
-- Forward-only, additive (NFR-5): nullable column, existing rows default to
-- NULL (= active). No backfill required.
ALTER TABLE projects
    ADD COLUMN archived_at TIMESTAMPTZ DEFAULT NULL;

-- Partial index to keep the active-registry listing fast as the project count
-- grows; archived rows are excluded from the index entirely.
CREATE INDEX projects_active_idx
    ON projects (created_at)
    WHERE archived_at IS NULL;
