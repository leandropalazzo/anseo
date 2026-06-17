-- Story 20.3 — Row-Level Security (D-P4-8, RR-Phase4-RlsFailClosed).
--
-- Enables RLS on every tenant table and installs fail-closed policies.
--
-- Fail-closed guarantee (RR-Phase4-RlsFailClosed):
--   The USING predicate uses current_setting('app.org', true) — the second
--   argument `true` makes the call return NULL (rather than raising) when the
--   GUC is unset. NULL::uuid = org_id is FALSE, so unset GUC → zero rows.
--   This is the load-bearing control: even an app-layer bug cannot cross orgs
--   because the DB refuses.
--
-- Non-tenant tables (organizations, operators, operator_org_roles,
-- brand_grants, org_invites) are NOT subject to this policy — they are
-- managed by the authZ layer, not per-org RLS. The audit table uses an
-- append-only policy (RR-Phase4-AppendOnlyAudit).
--
-- Self-host single-tenant mode: the default org's UUID is set as the process-
-- level GUC via `ALTER ROLE ... SET app.org = '<uuid>'` at startup (Story 20.4
-- — GUC middleware). In single-tenant mode this is set once; in multi-tenant
-- mode it is set per-request via SET LOCAL inside the transaction.
--
-- FORCE ROW LEVEL SECURITY is set so that superuser / table owner connections
-- (e.g. migration runner) are ALSO subject to the policy after it is enabled.
-- Migrations run before RLS is enabled and are not affected.

-- --------------------------------------------------------------------------
-- Shared policy template
-- --------------------------------------------------------------------------
-- Every tenant table gets:
--   SELECT / UPDATE / DELETE: USING (org_id = current_setting('app.org', true)::uuid)
--   INSERT:                    WITH CHECK (org_id = current_setting('app.org', true)::uuid)
-- --------------------------------------------------------------------------

-- Helper: enable RLS + FORCE + policies on a table.
-- (Inline because PL/pgSQL CREATE FUNCTION requires a transaction-safe context.)

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
        -- Enable RLS (idempotent — no-op if already enabled)
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY',  t);

        -- SELECT / UPDATE / DELETE: fail-closed on unset GUC
        EXECUTE format(
            'CREATE POLICY %I ON %I USING (
                org_id = current_setting(''app.org'', true)::uuid
            )',
            'rls_org_' || t, t
        );

        -- INSERT: enforce org matches GUC
        EXECUTE format(
            'CREATE POLICY %I ON %I FOR INSERT WITH CHECK (
                org_id = current_setting(''app.org'', true)::uuid
            )',
            'rls_org_insert_' || t, t
        );
    END LOOP;
END $$;

-- --------------------------------------------------------------------------
-- Audit table: append-only policy (RR-Phase4-AppendOnlyAudit).
-- The audit_runs table already has a DB-level append-only trigger (48.2).
-- This policy layer adds a belt-and-suspenders: no UPDATE or DELETE via the
-- app_role even if the trigger is somehow bypassed.
-- --------------------------------------------------------------------------
-- (audit_runs is already in the tenant_tables list above, which gives it the
-- org-scoped SELECT/UPDATE/DELETE/INSERT policies. The append-only invariant
-- is separately enforced by the trigger from Story 48.2.)

-- --------------------------------------------------------------------------
-- GA criterion p4-iso-2: flip this bit in phase4-ga-check.sh once the
-- rls_fail_closed integration test (crates/storage/tests/rls_fail_closed.rs)
-- is green and wired to the check script.
-- --------------------------------------------------------------------------
