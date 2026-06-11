-- Story 49.0 (D2) — terms-finalize gate: OSS-owned source of truth.
--
-- The terms-finalized toggle, the active benchmark terms_version, and the
-- k>=5 density floor live HERE, in OSS Postgres (apps/api / crates/storage) —
-- NOT in `anseo_admin`. An OSS consumer (the CLI `anseo benchmark optin`
-- path, the server-side ingest terms-version check) reads this table directly,
-- so OSS never reads `anseo_admin` (ADR-007 intact). The console only MIRRORS
-- these values into `anseo_admin.admin_settings` for display; the write path
-- back into OSS is the `PUT /v1/operator/config/benchmark-gate` operator
-- endpoint, whose effect is recorded here.
--
-- Single-row table: a fixed sentinel primary key (`'default'`) so there is
-- exactly one gate config for the deployment. The PUT upserts that row; the
-- GET reads it (returning a built-in default when the row is absent, so a
-- fresh deployment is readable before the console first writes).
--
-- Forward-only (ARCH D-2).

CREATE TABLE benchmark_gate_config (
    -- Fixed sentinel: there is exactly one gate config row per deployment.
    id              TEXT PRIMARY KEY DEFAULT 'default'
        CHECK (id = 'default'),
    -- The terms-finalize toggle. When false, the CLI optin path / ingest gate
    -- treat the benchmark as not-yet-open regardless of terms_version.
    terms_finalized BOOLEAN NOT NULL DEFAULT false,
    -- The active benchmark terms version the redactor / optin path pins against.
    terms_version   TEXT NOT NULL DEFAULT 'unset',
    -- The k>=5 density floor (minimum distinct contributors per segment). Stored
    -- so the console can tune it; defaults to the build-in floor (5).
    density_floor   INTEGER NOT NULL DEFAULT 5
        CHECK (density_floor >= 1),
    -- Last operator to write the gate (audit; echoed to the BFF), nullable.
    updated_by      TEXT NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
