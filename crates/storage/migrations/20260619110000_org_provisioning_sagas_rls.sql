-- AC-4 RLS guard: org_provisioning_sagas missed RLS in the 27.1 migration.
-- The rls_matrix test enforces FORCE ROW LEVEL SECURITY + policies on every
-- table with an org_id column. This migration backfills the requirement.

ALTER TABLE org_provisioning_sagas FORCE ROW LEVEL SECURITY;

CREATE POLICY rls_org_provisioning_sagas ON org_provisioning_sagas
    USING (org_id = current_setting('app.org', true)::uuid);

CREATE POLICY rls_org_insert_provisioning_sagas ON org_provisioning_sagas
    FOR INSERT
    WITH CHECK (org_id = current_setting('app.org', true)::uuid);
