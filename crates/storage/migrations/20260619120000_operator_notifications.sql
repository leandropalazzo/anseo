-- Story 27.3 — Transactional notification service + notification center.
--
-- operator_notifications stores account-affecting events (invites, dunning,
-- security alerts, cap warnings, anomaly alerts) per operator per org.
-- The frontend notification center polls / streams from this table.

CREATE TYPE notification_kind AS ENUM (
    'email_verification',
    'member_invite',
    'dunning_grace',
    'dunning_suspended',
    'security_alert',
    'cap_approaching',
    'anomaly_alert'
);

CREATE TABLE operator_notifications (
    id          UUID                    PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      UUID        NOT NULL    REFERENCES organizations (id) ON DELETE CASCADE,
    operator_id UUID                    REFERENCES operators (id) ON DELETE CASCADE,
    kind        notification_kind NOT NULL,
    subject     TEXT        NOT NULL,
    body_text   TEXT        NOT NULL    DEFAULT '',
    read_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL    DEFAULT now()
);

CREATE INDEX notif_org_idx      ON operator_notifications (org_id, created_at DESC);
CREATE INDEX notif_operator_idx ON operator_notifications (operator_id, read_at)
    WHERE operator_id IS NOT NULL;

-- AC-4 RLS guard: every table with org_id must have FORCE ROW LEVEL SECURITY.
ALTER TABLE operator_notifications FORCE ROW LEVEL SECURITY;

CREATE POLICY rls_org_operator_notifications ON operator_notifications
    USING (org_id = current_setting('app.org', true)::uuid);

CREATE POLICY rls_org_insert_operator_notifications ON operator_notifications
    FOR INSERT
    WITH CHECK (org_id = current_setting('app.org', true)::uuid);
