-- Story 20.2 follow-up — set org_id column defaults on all tenant tables.
--
-- The backfill migration (20260617210000) added org_id as NOT NULL (after
-- backfilling all existing rows). This migration installs a helper function
-- and wires it as the column DEFAULT so that INSERT statements that omit
-- org_id (single-tenant self-host mode; legacy code paths; tests) automatically
-- get the default org's UUID rather than a NULL-violation.
--
-- In multi-tenant hosted mode the GUC middleware always sets OrgContext, so the
-- application layer always supplies org_id explicitly. The DEFAULT here is a
-- belt-and-suspenders for single-tenant and test code.

-- Helper function: returns the UUID of the 'default' organization.
-- STABLE because it reads the DB but does not write.
CREATE OR REPLACE FUNCTION default_org_id() RETURNS UUID
    LANGUAGE SQL STABLE
AS $$
    SELECT id FROM organizations WHERE slug = 'default' LIMIT 1
$$;

-- Set the DEFAULT on every tenant table's org_id column.
DO $$
DECLARE
    t TEXT;
    tenant_tables TEXT[] := ARRAY[
        'projects',
        'prompts',
        'prompt_runs',
        'mentions',
        'citations',
        'api_keys',
        'webhooks',
        'webhook_deliveries',
        'notification_targets',
        'schedules',
        'schedule_ticks',
        'recommendations',
        'audit_runs',
        'anonymous_contributions',
        'benchmark_consent',
        'contributions',
        'alert_rules',
        'plugin_installs'
    ];
BEGIN
    FOREACH t IN ARRAY tenant_tables LOOP
        EXECUTE format(
            'ALTER TABLE %I ALTER COLUMN org_id SET DEFAULT default_org_id()',
            t
        );
    END LOOP;
END $$;
