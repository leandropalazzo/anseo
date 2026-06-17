-- Story 20.2 — org_id backfill + default-Org migration (D-P4-7).
--
-- Three-phase pattern (idempotent + resumable):
--   Phase A: Insert the single default Org (slug = 'default', idempotent on
--            conflict). Captures its id into a local variable for backfill.
--   Phase B: Add nullable `org_id FK` to every Phase 1–3 tenant table;
--            backfill all existing rows to the default Org.
--   Phase C: Set NOT NULL on each column now that every row has a value.
--
-- Invariants:
--   * Self-host single-tenant data remains fully readable post-migration.
--   * Phase 1 ARCH-4 forward-only invariant preserved.
--   * `/v1` payload shapes are untouched (brand_id alias is a view, not a
--     column rename — RR-Phase4-NoContractBreak).
--   * Re-running this migration is a no-op (INSERT ... ON CONFLICT DO NOTHING,
--     ADD COLUMN IF NOT EXISTS).
--
-- RLS note: RLS is NOT enabled here. That is Story 20.3. This migration only
-- adds the structural FK; the policy layer comes next.

-- --------------------------------------------------------------------------
-- Phase A: default Org
-- --------------------------------------------------------------------------
INSERT INTO organizations (slug, name)
VALUES ('default', 'Default Organization')
ON CONFLICT (slug) DO NOTHING;

-- --------------------------------------------------------------------------
-- Phase B: add org_id FK columns (nullable first, then backfill).
-- --------------------------------------------------------------------------
-- We use a DO block so we can reference the default org UUID by slug.
DO $$
DECLARE
    default_org_id UUID;
BEGIN
    SELECT id INTO default_org_id
    FROM organizations
    WHERE slug = 'default';

    -- projects (core Phase 1 tenant table)
    ALTER TABLE projects ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE projects SET org_id = default_org_id WHERE org_id IS NULL;

    -- prompts
    ALTER TABLE prompts ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE prompts SET org_id = default_org_id WHERE org_id IS NULL;

    -- prompt_runs
    ALTER TABLE prompt_runs ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE prompt_runs SET org_id = default_org_id WHERE org_id IS NULL;

    -- mentions
    ALTER TABLE mentions ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE mentions SET org_id = default_org_id WHERE org_id IS NULL;

    -- citations
    ALTER TABLE citations ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE citations SET org_id = default_org_id WHERE org_id IS NULL;

    -- api_keys (Phase 2)
    ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE api_keys SET org_id = default_org_id WHERE org_id IS NULL;

    -- webhooks (Phase 2)
    ALTER TABLE webhooks ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE webhooks SET org_id = default_org_id WHERE org_id IS NULL;

    -- webhook_deliveries (Phase 2)
    ALTER TABLE webhook_deliveries ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE webhook_deliveries SET org_id = default_org_id WHERE org_id IS NULL;

    -- notification_targets (Phase 2)
    ALTER TABLE notification_targets ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE notification_targets SET org_id = default_org_id WHERE org_id IS NULL;

    -- schedules (Phase 2)
    ALTER TABLE schedules ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE schedules SET org_id = default_org_id WHERE org_id IS NULL;

    -- schedule_ticks (Phase 2)
    ALTER TABLE schedule_ticks ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE schedule_ticks SET org_id = default_org_id WHERE org_id IS NULL;

    -- recommendations (Phase 3)
    ALTER TABLE recommendations ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE recommendations SET org_id = default_org_id WHERE org_id IS NULL;

    -- audit_runs (Phase 2/3 alert checks)
    ALTER TABLE audit_runs ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE audit_runs SET org_id = default_org_id WHERE org_id IS NULL;

    -- anonymous_contributions (Phase 4 run ingest)
    ALTER TABLE anonymous_contributions ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE anonymous_contributions SET org_id = default_org_id WHERE org_id IS NULL;

    -- benchmark_consent (Phase 3/4)
    ALTER TABLE benchmark_consent ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE benchmark_consent SET org_id = default_org_id WHERE org_id IS NULL;

    -- contributions (Phase 4 benchmark ingest)
    ALTER TABLE contributions ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE contributions SET org_id = default_org_id WHERE org_id IS NULL;

    -- alert_rules (Phase 3)
    ALTER TABLE alert_rules ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE alert_rules SET org_id = default_org_id WHERE org_id IS NULL;

    -- plugin_installs (Phase 3 plugin marketplace)
    ALTER TABLE plugin_installs ADD COLUMN IF NOT EXISTS org_id UUID
        REFERENCES organizations(id) ON DELETE RESTRICT;
    UPDATE plugin_installs SET org_id = default_org_id WHERE org_id IS NULL;

END $$;

-- --------------------------------------------------------------------------
-- Phase C: set NOT NULL now that every row has been backfilled.
-- --------------------------------------------------------------------------
ALTER TABLE projects               ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE prompts                ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE prompt_runs            ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE mentions               ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE citations              ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE api_keys               ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE webhooks               ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE webhook_deliveries     ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE notification_targets   ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE schedules              ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE schedule_ticks         ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE recommendations        ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE audit_runs             ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE anonymous_contributions ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE benchmark_consent      ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE contributions          ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE alert_rules            ALTER COLUMN org_id SET NOT NULL;
ALTER TABLE plugin_installs        ALTER COLUMN org_id SET NOT NULL;

-- --------------------------------------------------------------------------
-- Phase D: org_id index on the hot query tables.
-- --------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS projects_org_id_idx         ON projects (org_id);
CREATE INDEX IF NOT EXISTS prompts_org_id_idx          ON prompts (org_id);
CREATE INDEX IF NOT EXISTS prompt_runs_org_id_idx      ON prompt_runs (org_id);
CREATE INDEX IF NOT EXISTS api_keys_org_id_idx         ON api_keys (org_id);
CREATE INDEX IF NOT EXISTS schedules_org_id_idx        ON schedules (org_id);
CREATE INDEX IF NOT EXISTS recommendations_org_id_idx  ON recommendations (org_id);

-- --------------------------------------------------------------------------
-- Phase E: brand_id alias view (RR-Phase4-NoContractBreak).
-- Exposes project_id as brand_id for the Phase 4 authZ layer without
-- renaming the column or touching any /v1 payload shape.
-- --------------------------------------------------------------------------
CREATE OR REPLACE VIEW brands AS
SELECT
    id            AS id,
    id            AS brand_id,   -- alias: brand_id = project_id in Phase 4 model
    name,
    site_url,
    competitors,
    variants,
    org_id,
    created_at,
    archived_at
FROM projects;
