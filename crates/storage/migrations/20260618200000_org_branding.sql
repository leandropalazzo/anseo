-- Story 25.1 — per-org branding (logo, accent color, custom slug).
-- Plan-gated: Pro and Enterprise only (enforced at the API layer).

CREATE TABLE org_branding (
    org_id      UUID        PRIMARY KEY REFERENCES organizations (id) ON DELETE CASCADE,
    logo_url    TEXT        NULL,
    accent_hex  TEXT        NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- accent_hex must be a 7-char hex (#RRGGBB) or NULL.
    CONSTRAINT org_branding_accent_hex_format
        CHECK (accent_hex IS NULL OR accent_hex ~ '^#[0-9a-fA-F]{6}$')
);

ALTER TABLE org_branding ENABLE ROW LEVEL SECURITY;
ALTER TABLE org_branding FORCE ROW LEVEL SECURITY;

CREATE POLICY rls_org_org_branding ON org_branding
    USING (org_id = current_setting('app.org', true)::uuid);
CREATE POLICY rls_org_insert_org_branding ON org_branding
    FOR INSERT
    WITH CHECK (org_id = current_setting('app.org', true)::uuid);
