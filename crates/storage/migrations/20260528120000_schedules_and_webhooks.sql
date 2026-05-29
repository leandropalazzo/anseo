-- OpenGEO Phase 2 — Scheduler, worker, webhooks, and notifications.
--
-- ARCH-21: at-most-once via `INSERT ... ON CONFLICT DO NOTHING` on
--   `schedule_ticks (schedule_id, tick_ts)`. Orphan reaper marks abandoned
--   `claimed` rows as `rolled_forward` after 5 min idle.
-- ARCH-22: Schedule density caps live in code (crates/scheduler/src/caps.rs),
--   not the schema; cap violation is a declare-time CLI/API error.
-- ARCH-23: Per-schedule projected monthly cost is recorded at declare time;
--   `projection_acknowledged_at` proves the user ack'd above-cap projections.
-- Forward-only discipline (D-2): no DROP / ALTER … DROP COLUMN here.
-- Phase 4 tenant fields (organization_id / tenant_id) are NULL placeholders
--   per Phase 1 Epic 1 convention.

CREATE TABLE schedules (
    id                              UUID PRIMARY KEY,
    project_id                      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    name                            TEXT NOT NULL,
    cron                            TEXT NOT NULL,
    prompts                         JSONB NOT NULL,
    providers                       JSONB NOT NULL,
    debounce_minutes                INT  NOT NULL DEFAULT 5,
    projected_monthly_usd           DOUBLE PRECISION NULL,
    projection_acknowledged_at      TIMESTAMPTZ NULL,
    paused                          BOOLEAN NOT NULL DEFAULT FALSE,
    organization_id                 UUID NULL,
    tenant_id                       UUID NULL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, name)
);

CREATE TABLE schedule_ticks (
    id                              UUID PRIMARY KEY,
    schedule_id                     UUID NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    tick_ts                         TIMESTAMPTZ NOT NULL,
    status                          TEXT NOT NULL CHECK (status IN (
        'planned',
        'claimed',
        'completed',
        'failed',
        'capped',
        'rolled_forward',
        'missed',
        'debounced'
    )),
    claimed_by                      TEXT NULL,
    claimed_at                      TIMESTAMPTZ NULL,
    completed_at                    TIMESTAMPTZ NULL,
    error_message                   TEXT NULL,
    organization_id                 UUID NULL,
    tenant_id                       UUID NULL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (schedule_id, tick_ts)
);

CREATE TABLE webhooks (
    id                              UUID PRIMARY KEY,
    project_id                      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    name                            TEXT NOT NULL,
    target_url                      TEXT NOT NULL,
    secret_ciphertext               TEXT NOT NULL,
    event_kinds                     JSONB NOT NULL,
    disabled                        BOOLEAN NOT NULL DEFAULT FALSE,
    disabled_reason                 TEXT NULL,
    organization_id                 UUID NULL,
    tenant_id                       UUID NULL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, name)
);

CREATE TABLE webhook_deliveries (
    id                              UUID PRIMARY KEY,
    webhook_id                      UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_id                        UUID NOT NULL,
    event_kind                      TEXT NOT NULL,
    attempt                         INT NOT NULL,
    status                          TEXT NOT NULL CHECK (status IN (
        'pending',
        'delivered',
        'failed',
        'dropped'
    )),
    response_status                 INT NULL,
    response_body_snippet           TEXT NULL,
    next_attempt_at                 TIMESTAMPTZ NULL,
    organization_id                 UUID NULL,
    tenant_id                       UUID NULL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE notification_targets (
    id                              UUID PRIMARY KEY,
    project_id                      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    name                            TEXT NOT NULL,
    channel                         TEXT NOT NULL CHECK (channel IN ('slack', 'smtp')),
    config                          JSONB NOT NULL,
    event_kinds                     JSONB NOT NULL,
    disabled                        BOOLEAN NOT NULL DEFAULT FALSE,
    organization_id                 UUID NULL,
    tenant_id                       UUID NULL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, name)
);

-- Indexes per ARCH-21 (claim path + reaper sweep) and ARCH-16 (SSE replay).
CREATE INDEX idx_schedules_project_id              ON schedules (project_id);
CREATE INDEX idx_schedule_ticks_schedule_status    ON schedule_ticks (schedule_id, status);
CREATE INDEX idx_schedule_ticks_status_claimed_at  ON schedule_ticks (status, claimed_at);
CREATE INDEX idx_schedule_ticks_tick_ts            ON schedule_ticks (tick_ts);
CREATE INDEX idx_webhooks_project_id               ON webhooks (project_id);
CREATE INDEX idx_webhook_deliveries_webhook        ON webhook_deliveries (webhook_id, created_at);
CREATE INDEX idx_webhook_deliveries_pending        ON webhook_deliveries (status, next_attempt_at)
    WHERE status = 'pending';
CREATE INDEX idx_notification_targets_project_id   ON notification_targets (project_id);

-- Phase 2 forward-compat for the Phase 1 prompt_runs CHECK on error_kind.
-- Story 11.1 ships the `provider_unsupported_model` variant; we widen the
-- constraint here (additive only) so 11.1 can land without a schema migration.
ALTER TABLE prompt_runs DROP CONSTRAINT prompt_runs_error_kind_check;
ALTER TABLE prompt_runs ADD  CONSTRAINT prompt_runs_error_kind_check CHECK (
    error_kind IS NULL OR error_kind IN (
        'provider_unauthorized',
        'provider_rate_limited',
        'provider_timeout',
        'provider_5xx',
        'provider_invalid_response',
        'network_error',
        'provider_unsupported_model'
    )
);

-- Cross-reference between a Prompt Run and the Schedule tick that produced it
-- (NULL = manual `ogeo prompt run`). Additive column per ARCH-4.
ALTER TABLE prompt_runs ADD COLUMN schedule_tick_id UUID NULL
    REFERENCES schedule_ticks(id) ON DELETE SET NULL;
CREATE INDEX idx_prompt_runs_schedule_tick_id ON prompt_runs (schedule_tick_id);
