-- Story 25.2 — custom domain state machine columns on org_branding.
-- [mock-OK] Real ACM/CloudFront provisioning is handled at a later story.

ALTER TABLE org_branding
    ADD COLUMN IF NOT EXISTS custom_domain      TEXT        NULL,
    ADD COLUMN IF NOT EXISTS domain_status      TEXT        NOT NULL DEFAULT 'unclaimed'
        CONSTRAINT domain_status_check CHECK (domain_status IN (
            'unclaimed','pending_verification','verified','provisioning','active','failed'
        )),
    ADD COLUMN IF NOT EXISTS domain_txt_record  TEXT        NULL,
    ADD COLUMN IF NOT EXISTS domain_verified_at TIMESTAMPTZ NULL,
    ADD COLUMN IF NOT EXISTS tls_status         TEXT        NOT NULL DEFAULT 'none'
        CONSTRAINT tls_status_check CHECK (tls_status IN ('none','provisioning','active','failed'));
