-- OpenGEO Phase 2 Story 12.1 — API key authentication for the REST surface.
--
-- Keys are issued by `ogeo api key create`. The plaintext is shown ONCE at
-- create time; the row stores only the sha256 hash. A short `prefix` is kept
-- in cleartext (the first 8 chars of the key after the `ogeo_` literal) so
-- the dashboard / `list` command can show "ogeo_AbCdEfGh…" identifiers
-- without revealing the secret.
--
-- Revocation is soft (revoked_at IS NOT NULL). The auth middleware excludes
-- revoked rows; old hashes stick around for audit.

CREATE TABLE api_keys (
    id                              UUID PRIMARY KEY,
    project_id                      UUID NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    name                            TEXT NOT NULL,
    -- sha256 of the issued plaintext, lowercase hex (64 chars).
    sha256_hash                     TEXT NOT NULL UNIQUE,
    -- First 8 chars of the random portion (after `ogeo_`) — safe to display.
    prefix                          TEXT NOT NULL,
    last_used_at                    TIMESTAMPTZ NULL,
    revoked_at                      TIMESTAMPTZ NULL,
    revoked_reason                  TEXT NULL,
    organization_id                 UUID NULL,
    tenant_id                       UUID NULL,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_api_keys_project_id    ON api_keys (project_id);
CREATE INDEX idx_api_keys_sha256_active ON api_keys (sha256_hash) WHERE revoked_at IS NULL;
-- Partial uniqueness: only ACTIVE keys collide on (project_id, name). A
-- revoked row keeps its (project_id, name) slot in the table for audit, but
-- a follow-up `ogeo api key create --name <same>` can re-issue without
-- hitting a UNIQUE violation.
CREATE UNIQUE INDEX idx_api_keys_project_name_active
    ON api_keys (project_id, name)
    WHERE revoked_at IS NULL;
