-- Story 24.1 — per-org billing entitlements synchronized from Stripe.
--
-- One row per org. Stripe identifiers are nullable so self-host/default
-- organizations can still carry a Free entitlement without an external customer.

CREATE TABLE org_entitlements (
    id                     UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id                 UUID        NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    plan                   TEXT        NOT NULL,
    seat_count             INT         NOT NULL,
    stripe_customer_id     TEXT        NULL UNIQUE,
    stripe_subscription_id TEXT        NULL,
    synced_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT org_entitlements_org_unique UNIQUE (org_id),
    CONSTRAINT org_entitlements_plan_check CHECK (plan IN ('free', 'pro', 'enterprise')),
    CONSTRAINT org_entitlements_seat_count_check CHECK (seat_count >= 0)
);

CREATE INDEX org_entitlements_org_idx ON org_entitlements (org_id);
CREATE INDEX org_entitlements_subscription_idx
    ON org_entitlements (stripe_subscription_id)
    WHERE stripe_subscription_id IS NOT NULL;

ALTER TABLE org_entitlements ENABLE ROW LEVEL SECURITY;
ALTER TABLE org_entitlements FORCE ROW LEVEL SECURITY;

CREATE POLICY rls_org_org_entitlements ON org_entitlements
    USING (org_id = current_setting('app.org', true)::uuid);

CREATE POLICY rls_org_insert_org_entitlements ON org_entitlements
    FOR INSERT
    WITH CHECK (org_id = current_setting('app.org', true)::uuid);
