-- OIDC SSO connector configs per org (one active at a time per org).
-- Actual Cognito/AWS provisioning is deferred (mock-OK); this stores the config
-- so the API surface and flow exist for testing.
CREATE TABLE oidc_sso_connectors (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id          UUID        NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    provider        TEXT        NOT NULL, -- 'generic_oidc' | 'google_workspace' | 'okta'
    client_id       TEXT        NOT NULL,
    -- client_secret stored encrypted via KmsOrgStore (never plaintext in DB)
    client_secret_ref TEXT      NOT NULL,  -- opaque ref into KMS DEK-wrapped store
    issuer_url      TEXT        NOT NULL,  -- e.g. https://cognito-idp.us-east-1.amazonaws.com/us-east-xxx
    redirect_uri    TEXT        NOT NULL,
    enabled         BOOLEAN     NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT oidc_provider_check CHECK (provider IN ('generic_oidc','google_workspace','okta'))
);
CREATE INDEX oidc_sso_org_idx ON oidc_sso_connectors (org_id);
-- RLS: only the owning org can see/write its connectors
ALTER TABLE oidc_sso_connectors ENABLE ROW LEVEL SECURITY;
ALTER TABLE oidc_sso_connectors FORCE ROW LEVEL SECURITY;
CREATE POLICY oidc_sso_select ON oidc_sso_connectors
    FOR SELECT USING (org_id = current_setting('app.org', true)::uuid);
CREATE POLICY oidc_sso_insert ON oidc_sso_connectors
    FOR INSERT WITH CHECK (org_id = current_setting('app.org', true)::uuid);
CREATE POLICY oidc_sso_update ON oidc_sso_connectors
    FOR UPDATE USING (org_id = current_setting('app.org', true)::uuid);
CREATE POLICY oidc_sso_delete ON oidc_sso_connectors
    FOR DELETE USING (org_id = current_setting('app.org', true)::uuid);
