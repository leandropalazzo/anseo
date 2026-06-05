-- Epic 43 / Story 43.3 — Entity dedup + collision handling tables.
--
-- dedup_review_queue: ambiguous near-duplicate candidates that fall below the
-- auto-merge confidence threshold. Operator must manually adjudicate.
--
-- False-merge is worse than false-split: a false merge bleeds one brand's
-- badge/visibility into another (defamation/false-endorsement vector).
-- Default is DO NOT auto-merge — ambiguous cases go here.
--
-- verification_attempts: append-only log of every claim attempt (DNS-TXT +
-- email magic-link). Written by 43.2 flow; referenced by 43.3 conflict logic.

CREATE TABLE verification_attempts (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    domain              TEXT NOT NULL REFERENCES entities(domain) ON DELETE CASCADE,
    token_hash          TEXT NOT NULL,  -- SHA-256 of the raw token (never store raw)
    method              TEXT NOT NULL
        CHECK (method IN ('dns_txt', 'email_magic_link')),
    status              TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'verified', 'failed', 'expired', 'replayed')),
    claimant_email      TEXT NULL,
    attestation_version TEXT NOT NULL DEFAULT 'v1',
    attested_at         TIMESTAMPTZ NULL,
    expires_at          TIMESTAMPTZ NOT NULL,
    used_at             TIMESTAMPTZ NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_verification_attempts_domain
    ON verification_attempts (domain, created_at DESC);
CREATE INDEX idx_verification_attempts_token_hash
    ON verification_attempts (token_hash)
    WHERE status = 'pending';

-- dedup_review_queue: human-in-the-loop adjudication for ambiguous merges.
CREATE TABLE dedup_review_queue (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    canonical_domain    TEXT NOT NULL REFERENCES entities(domain) ON DELETE CASCADE,
    candidate_domain    TEXT NOT NULL,
    candidate_name      TEXT NOT NULL,
    similarity_score    SMALLINT NOT NULL CHECK (similarity_score BETWEEN 0 AND 100),
    match_reason        TEXT NOT NULL,  -- e.g. 'homoglyph', 'near_dup', 'cross_operator'
    status              TEXT NOT NULL DEFAULT 'pending_review'
        CHECK (status IN (
            'pending_review', 'approved_merge', 'rejected_merge', 'escalated'
        )),
    reviewed_by         TEXT NULL,   -- operator identifier
    reviewed_at         TIMESTAMPTZ NULL,
    rationale           TEXT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_dedup_review_queue_status
    ON dedup_review_queue (status, created_at DESC)
    WHERE status = 'pending_review';
