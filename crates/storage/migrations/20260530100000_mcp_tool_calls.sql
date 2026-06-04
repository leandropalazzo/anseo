-- MCP tool call activity log. Populated by opengeo-mcp when it processes
-- a tool call (Story 16.8). Queried by the dashboard for activity log
-- and per-tool analytics (Story 16.9).
CREATE TABLE IF NOT EXISTS mcp_tool_calls (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tool_name       TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('ok', 'error')),
    latency_ms      INTEGER NOT NULL,
    error_kind      TEXT,
    called_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX mcp_tool_calls_tool_name_idx ON mcp_tool_calls (tool_name, called_at DESC);
CREATE INDEX mcp_tool_calls_called_at_idx ON mcp_tool_calls (called_at DESC);
