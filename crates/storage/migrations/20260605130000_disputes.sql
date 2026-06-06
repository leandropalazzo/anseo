-- Epic 43 / Story 43.6 — Full disputes lifecycle.
--
-- Builds on the entity registry (20260605100000) and dedup/verification tables
-- (20260605110000). The minimal submission path (42.6) writes a dispute row;
-- this story adds the operator-review workflow, claim-conflict adjudication
-- (DNS-TXT control is the sole arbiter), GDPR Art.21 objection assessment,
-- change-of-control transfer, and a removal/suppression workflow.
--
-- BD-3: nothing in this flow touches billing or premium tiers.
--
-- Two tables:
--   disputes        — one row per request (correction / claim-conflict /
--                     gdpr-objection / removal / change-of-control).
--   dispute_events  — append-only audit log: every state change is recorded
--                     with actor, timestamp, and rationale (NFR5 auditability).
--
-- Forward-only (ARCH D-2).

CREATE TABLE disputes (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Subject domain (already normalized by the application layer). Not a FK:
    -- a dispute may reference a domain that has no registry row yet (an entity
    -- appearing on the benchmark before it is registered).
    domain              TEXT NOT NULL,
    dispute_type        TEXT NOT NULL
        CHECK (dispute_type IN (
            'correction',          -- factual-error correction request (AC-1)
            'claim_conflict',      -- two parties asserting the same domain (AC-2)
            'gdpr_objection',      -- GDPR Art.21 objection on personal data (AC-3)
            'removal',             -- removal/suppression request (AC-4)
            'change_of_control'    -- domain ownership transfer arbitration
        )),
    status              TEXT NOT NULL DEFAULT 'open'
        CHECK (status IN (
            'open',                -- submitted, awaiting operator review
            'under_review',        -- operator picked it up
            'approved',            -- correction approved / objection honored / etc.
            'rejected',            -- refused with grounds
            'resolved'             -- terminal: conflict adjudicated / control transferred
        )),
    -- Free-text submitter-provided description of the issue / grounds.
    description         TEXT NOT NULL DEFAULT '',
    -- Optional contact for notifying the submitter / losing party.
    submitter_email     TEXT NULL,
    -- For corrections: the proposed new display_name (nullable for other types).
    proposed_value      TEXT NULL,
    -- Suppression flag: when true, the domain is hidden from public display
    -- pending review (AC-4). Mirrored onto the entity registry by the app layer.
    suppressed          BOOLEAN NOT NULL DEFAULT FALSE,
    -- Operator decision metadata.
    resolved_by         TEXT NULL,   -- operator identifier
    resolved_at         TIMESTAMPTZ NULL,
    resolution_grounds  TEXT NULL,   -- plain-language reason (NFR1 transparency)
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_disputes_domain ON disputes (domain, created_at DESC);
-- Operator queue: open + under_review, oldest first.
CREATE INDEX idx_disputes_open
    ON disputes (status, created_at ASC)
    WHERE status IN ('open', 'under_review');
-- Suppression lookup for public-display filtering.
CREATE INDEX idx_disputes_suppressed
    ON disputes (domain)
    WHERE suppressed = TRUE;

-- Append-only audit log. NEVER updated or deleted — every state change is a
-- new row (NFR5). `detail` carries a JSON blob with type-specific context
-- (e.g. winning/losing claimant, assessment outcome, prior/next status).
CREATE TABLE dispute_events (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dispute_id          UUID NOT NULL REFERENCES disputes(id) ON DELETE CASCADE,
    event_type          TEXT NOT NULL,  -- 'submitted','review_started','approved',
                                        -- 'rejected','suppressed','unsuppressed',
                                        -- 'conflict_adjudicated','control_transferred',
                                        -- 'gdpr_assessed','notification_sent'
    actor               TEXT NOT NULL DEFAULT 'system',  -- operator id or 'system'/'submitter'
    rationale           TEXT NULL,
    detail              JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_dispute_events_dispute
    ON dispute_events (dispute_id, created_at ASC);
