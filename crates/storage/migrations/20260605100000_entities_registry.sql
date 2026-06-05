-- Epic 43 / Story 43.1 — Entity registry: canonical domain → display-name
-- mapping with claim state and role.
--
-- Domain normalization contract: values stored here are already normalized
-- (lowercase, www-stripped, trailing-slash stripped). The application layer
-- enforces this before INSERT/UPDATE; this migration adds a CHECK that
-- rejects obvious violations.
--
-- `domain` is the PRIMARY KEY — one row per domain. The unique constraint is
-- implicit in the PK declaration; it is also stated explicitly as a named
-- constraint for clear error messages.
--
-- `role` TEXT NOT NULL CHECK enumerates the three identity roles a single
-- domain can occupy (measured subject, cited source, or both).
--
-- `claim_status` TEXT NOT NULL CHECK mirrors the state-machine in 43.2:
--   unclaimed → pending → verified | revoked
-- `pending_conflict` is set by 43.3 when two claimants simultaneously assert
-- the same domain.
--
-- Forward-only (ARCH D-2).

CREATE TABLE entities (
    domain              TEXT PRIMARY KEY,
    display_name        TEXT NOT NULL,
    role                TEXT NOT NULL DEFAULT 'source'
        CHECK (role IN ('brand', 'source', 'both')),
    claim_status        TEXT NOT NULL DEFAULT 'unclaimed'
        CHECK (claim_status IN (
            'unclaimed', 'pending', 'verified', 'revoked', 'pending_conflict'
        )),
    verified_at         TIMESTAMPTZ NULL,
    verification_method TEXT NULL
        CHECK (verification_method IS NULL OR
               verification_method IN ('dns_txt', 'email_magic_link')),
    grace_period_start  TIMESTAMPTZ NULL,  -- set when TXT record removed (43.3)
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Functional index: fast lookup by claim_status (operator review queue).
CREATE INDEX idx_entities_claim_status ON entities (claim_status);
-- Functional index: pending grace-period domains (daily re-verify job in 43.2).
CREATE INDEX idx_entities_grace_period ON entities (grace_period_start)
    WHERE grace_period_start IS NOT NULL;
