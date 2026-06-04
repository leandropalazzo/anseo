CREATE TABLE crawler_events (
    id BIGSERIAL PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    ts TIMESTAMPTZ NOT NULL,
    bot_id TEXT NOT NULL,
    path TEXT NOT NULL,
    status INTEGER NOT NULL CHECK (status >= 100 AND status <= 599),
    source_adapter TEXT NOT NULL,
    raw_event_id TEXT NOT NULL,
    ip_verified BOOLEAN NOT NULL DEFAULT FALSE,
    region TEXT,
    client_ip TEXT,
    client_ip_truncated TEXT,
    client_ip_hash TEXT,
    privacy_mode TEXT NOT NULL CHECK (privacy_mode IN ('raw', 'truncated', 'hashed')),
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT crawler_events_source_raw_unique UNIQUE (source_adapter, raw_event_id),
    CONSTRAINT crawler_events_privacy_ip_shape CHECK (
        (privacy_mode = 'raw' AND client_ip IS NOT NULL AND client_ip_hash IS NULL AND client_ip_truncated IS NULL)
        OR (privacy_mode = 'hashed' AND client_ip IS NULL AND client_ip_hash IS NOT NULL AND client_ip_truncated IS NULL)
        OR (privacy_mode = 'truncated' AND client_ip IS NULL AND client_ip_hash IS NULL AND client_ip_truncated IS NOT NULL)
        OR (client_ip IS NULL AND client_ip_hash IS NULL AND client_ip_truncated IS NULL)
    )
);

CREATE INDEX crawler_events_project_ts_idx ON crawler_events (project_id, ts DESC);
CREATE INDEX crawler_events_project_verified_ts_idx ON crawler_events (project_id, ip_verified, ts DESC);
CREATE INDEX crawler_events_project_bot_ts_idx ON crawler_events (project_id, bot_id, ts DESC);
