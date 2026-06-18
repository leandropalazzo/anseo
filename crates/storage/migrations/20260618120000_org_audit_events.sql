-- Story 26.1 — Actor-attributed append-only org audit event log.
--
-- Records every privileged org-management action (org creation, role grants/
-- revokes, brand grants, key ops) with actor attribution. Append-only is
-- enforced at the DB layer via BEFORE UPDATE/DELETE/TRUNCATE triggers so no
-- application code path can mutate or erase a record.

CREATE TABLE org_audit_events (
    id          BIGINT      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ts          TIMESTAMPTZ NOT NULL DEFAULT now(),
    org_id      UUID        NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    -- operator_id is null when the action is taken by a system process (e.g. bootstrap).
    operator_id UUID        NULL     REFERENCES operators (id) ON DELETE SET NULL,
    -- actor_login is the human-readable identity (GitHub login, API key prefix, or "system").
    actor_login TEXT        NOT NULL DEFAULT 'system',
    -- Dotted action key, e.g. "org.create", "org.role.grant", "brand.grant".
    action      TEXT        NOT NULL CHECK (length(btrim(action)) > 0),
    -- Optional secondary subject (domain, operator_id, project_id, etc.).
    target      TEXT        NULL,
    -- Non-sensitive structured context. Secrets are stripped before insert.
    metadata    JSONB       NULL
);

-- Covering indexes for the three common query shapes: org timeline, actor
-- history, and action-filtered search.
CREATE INDEX org_audit_events_org_ts_idx    ON org_audit_events (org_id, ts DESC);
CREATE INDEX org_audit_events_actor_ts_idx  ON org_audit_events (actor_login, ts DESC);
CREATE INDEX org_audit_events_action_ts_idx ON org_audit_events (action, ts DESC);

-- ── Append-only enforcement ────────────────────────────────────────────────

CREATE FUNCTION org_audit_events_immutable()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'org_audit_events is append-only: UPDATE/DELETE/TRUNCATE are not permitted';
END;
$$;

CREATE TRIGGER org_audit_events_no_update
    BEFORE UPDATE ON org_audit_events
    FOR EACH ROW EXECUTE FUNCTION org_audit_events_immutable();

CREATE TRIGGER org_audit_events_no_delete
    BEFORE DELETE ON org_audit_events
    FOR EACH ROW EXECUTE FUNCTION org_audit_events_immutable();

CREATE TRIGGER org_audit_events_no_truncate
    BEFORE TRUNCATE ON org_audit_events
    FOR EACH STATEMENT EXECUTE FUNCTION org_audit_events_immutable();
