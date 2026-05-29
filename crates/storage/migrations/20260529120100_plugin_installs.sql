-- Story 0.12 — Plugin install audit table for Epic 19 (Plugin SDK). See
-- `_bmad-output/planning-artifacts/architecture-phase3-plugin-sdk.md` §3
-- (install + audit) for the source-of-truth shape.
--
-- Append-mostly audit log of every plugin install / uninstall event. The
-- TOFU (trust on first use) signing model means we MUST be able to
-- reconstruct what was trusted, when, and by whom — even after the
-- plugin is removed. Hence `removed_at` is a soft-delete column; rows
-- are never DELETEd.
--
-- Security: we store ONLY the publisher pubkey FINGERPRINT (sha256 of
-- the DER-encoded pubkey, lowercase hex), never the full pubkey. The
-- full pubkey lives in the plugin manifest / package; this table is the
-- audit trail, not a key store. Logging the full key here would
-- duplicate state and risk leaking it via accidental dumps.
--
-- `capability_set` is a JSONB array of strings drawn from the closed
-- capability catalog defined in the Plugin SDK arch (§3). Free-form
-- JSONB rather than a normalised side table because the catalog is
-- code-versioned, not data-versioned — the set of valid strings is
-- enforced at install time by the SDK, not by the DB.
--
-- `installed_by_actor` is currently always `'local'` (single-tenant
-- Phase 3) but is a TEXT column so multi-tenant deployments can later
-- record operator identity without a migration.

CREATE TABLE plugin_installs (
    id                              UUID PRIMARY KEY,
    plugin_name                     TEXT NOT NULL,
    plugin_version                  TEXT NOT NULL,
    -- sha256 of the DER-encoded publisher pubkey, lowercase hex (64 chars).
    publisher_pubkey_fingerprint    TEXT NOT NULL,
    installed_at                    TIMESTAMPTZ NOT NULL DEFAULT now(),
    installed_by_actor              TEXT NOT NULL DEFAULT 'local',
    capability_set                  JSONB NOT NULL DEFAULT '[]'::jsonb,
    signature_verified              BOOLEAN NOT NULL,
    -- TOFU pin reference: the trust-root identifier under which this
    -- publisher's fingerprint was first pinned. NULL would mean
    -- unpinned, but the SDK refuses to install in that case so the
    -- column is NOT NULL.
    signing_trust_root              TEXT NOT NULL,
    removed_at                      TIMESTAMPTZ NULL,
    removed_reason                  TEXT NULL
);

CREATE INDEX idx_plugin_installs_name_installed_at
    ON plugin_installs (plugin_name, installed_at DESC);
-- Partial index: most rows are active (removed_at IS NULL); the
-- "what was uninstalled and why" audit query benefits from a tight
-- index over removed rows only.
CREATE INDEX idx_plugin_installs_removed_at
    ON plugin_installs (removed_at)
    WHERE removed_at IS NOT NULL;
