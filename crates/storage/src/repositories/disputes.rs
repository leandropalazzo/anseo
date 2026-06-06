//! Disputes repository — Story 43.6 (full disputes lifecycle).
//!
//! Builds on the entity registry (43.1), verification attempts + dedup queue
//! (43.3), and the minimal submission path (42.6). This module owns the
//! operator-review workflow, claim-conflict adjudication, GDPR Art.21
//! objection assessment, change-of-control transfer, and removal/suppression.
//!
//! # Invariants
//!
//! * **Domain control is the sole arbiter.** Claim conflicts are decided by
//!   DNS-TXT proof: the party that can produce the verified TXT record wins
//!   ([`DisputeRepo::adjudicate_claim_conflict`]). The application layer
//!   establishes the proof (via the 43.2 verification flow); this repo records
//!   the adjudicated outcome.
//! * **Every state change is audited.** No status transition happens without a
//!   corresponding `dispute_events` row (NFR5). The repo writes the audit row
//!   in the same transaction as the state change.
//! * **BD-3.** Nothing here references billing or premium tiers.
//!
//! Dynamic sqlx only (`sqlx::query` / `query_as::<_, Row>`) — no `query!`
//! macros (project HARD RULE).

use sqlx::PgPool;
use sqlx::Row as _;

use crate::error::Error;

/// A dispute record.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct DisputeRecord {
    pub id: uuid::Uuid,
    pub domain: String,
    pub dispute_type: String,
    pub status: String,
    pub description: String,
    pub submitter_email: Option<String>,
    pub proposed_value: Option<String>,
    pub suppressed: bool,
    pub resolved_by: Option<String>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolution_grounds: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// An append-only audit-log entry for a dispute.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct DisputeEvent {
    pub id: uuid::Uuid,
    pub dispute_id: uuid::Uuid,
    pub event_type: String,
    pub actor: String,
    pub rationale: Option<String>,
    pub detail: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// The set of accepted dispute types. The application layer validates against
/// this list before insert so invalid input fails with a clear 400 rather than
/// a DB CHECK violation.
pub const DISPUTE_TYPES: &[&str] = &[
    "correction",
    "claim_conflict",
    "gdpr_objection",
    "removal",
    "change_of_control",
];

/// Borrowing repository over a shared pool.
pub struct DisputeRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> DisputeRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    // -------------------------------------------------------------------------
    // Submission (extends the 42.6 minimal path)
    // -------------------------------------------------------------------------

    /// Create a dispute and write the `submitted` audit event atomically.
    ///
    /// For `removal` requests we also set `suppressed = TRUE` immediately so the
    /// domain is hidden from public display *pending* operator review (AC-4).
    /// The caller is responsible for normalizing `domain` first.
    pub async fn submit(
        &self,
        domain: &str,
        dispute_type: &str,
        description: &str,
        submitter_email: Option<&str>,
        proposed_value: Option<&str>,
    ) -> Result<DisputeRecord, Error> {
        let suppress_on_submit = dispute_type == "removal";

        let mut tx = self.pool.begin().await?;

        let id: uuid::Uuid = sqlx::query(
            r#"
            INSERT INTO disputes
                (domain, dispute_type, description, submitter_email,
                 proposed_value, suppressed)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(domain)
        .bind(dispute_type)
        .bind(description)
        .bind(submitter_email)
        .bind(proposed_value)
        .bind(suppress_on_submit)
        .fetch_one(&mut *tx)
        .await?
        .get("id");

        sqlx::query(
            r#"
            INSERT INTO dispute_events (dispute_id, event_type, actor, detail)
            VALUES ($1, 'submitted', 'submitter', $2)
            "#,
        )
        .bind(id)
        .bind(serde_json::json!({
            "dispute_type": dispute_type,
            "suppressed_on_submit": suppress_on_submit,
        }))
        .execute(&mut *tx)
        .await?;

        if suppress_on_submit {
            sqlx::query(
                r#"
                INSERT INTO dispute_events (dispute_id, event_type, actor, rationale)
                VALUES ($1, 'suppressed', 'system', 'auto-suppressed pending review (AC-4)')
                "#,
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.get(id).await?.ok_or(Error::NotFound)
    }

    /// Fetch a single dispute by id.
    pub async fn get(&self, id: uuid::Uuid) -> Result<Option<DisputeRecord>, Error> {
        let row = sqlx::query_as::<_, DisputeRecord>(
            r#"
            SELECT id, domain, dispute_type, status, description, submitter_email,
                   proposed_value, suppressed, resolved_by, resolved_at,
                   resolution_grounds, created_at, updated_at
            FROM disputes
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?;
        Ok(row)
    }

    /// Operator review queue: open + under_review disputes, oldest first.
    pub async fn pending(&self) -> Result<Vec<DisputeRecord>, Error> {
        let rows = sqlx::query_as::<_, DisputeRecord>(
            r#"
            SELECT id, domain, dispute_type, status, description, submitter_email,
                   proposed_value, suppressed, resolved_by, resolved_at,
                   resolution_grounds, created_at, updated_at
            FROM disputes
            WHERE status IN ('open', 'under_review')
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    /// Return the audit trail for a dispute, oldest first (NFR5).
    pub async fn events(&self, dispute_id: uuid::Uuid) -> Result<Vec<DisputeEvent>, Error> {
        let rows = sqlx::query_as::<_, DisputeEvent>(
            r#"
            SELECT id, dispute_id, event_type, actor, rationale, detail, created_at
            FROM dispute_events
            WHERE dispute_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(dispute_id)
        .fetch_all(self.pool)
        .await?;
        Ok(rows)
    }

    // -------------------------------------------------------------------------
    // Correction review (AC-1)
    // -------------------------------------------------------------------------

    /// Approve a factual-error correction: apply `proposed_value` to the entity
    /// registry's `display_name` (when present), mark the dispute approved, and
    /// log the decision. All in one transaction.
    ///
    /// Returns the updated dispute record. The notification to the submitter is
    /// the caller's responsibility (recorded via [`Self::record_notification`]).
    pub async fn approve_correction(
        &self,
        dispute_id: uuid::Uuid,
        operator: &str,
        rationale: &str,
    ) -> Result<DisputeRecord, Error> {
        let mut tx = self.pool.begin().await?;

        // Load the dispute to obtain domain + proposed_value.
        let row = sqlx::query(
            r#"SELECT domain, proposed_value FROM disputes WHERE id = $1 AND dispute_type = 'correction'"#,
        )
        .bind(dispute_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or(Error::NotFound)?;
        let domain: String = row.get("domain");
        let proposed: Option<String> = row.get("proposed_value");

        if let Some(new_name) = proposed.as_deref().filter(|s| !s.trim().is_empty()) {
            sqlx::query(
                r#"UPDATE entities SET display_name = $2, updated_at = now() WHERE domain = $1"#,
            )
            .bind(&domain)
            .bind(new_name.trim())
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            UPDATE disputes
            SET status = 'approved', resolved_by = $2, resolved_at = now(),
                resolution_grounds = $3, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(dispute_id)
        .bind(operator)
        .bind(rationale)
        .execute(&mut *tx)
        .await?;

        self.insert_event(
            &mut tx,
            dispute_id,
            "approved",
            operator,
            Some(rationale),
            serde_json::json!({ "applied_value": proposed }),
        )
        .await?;

        tx.commit().await?;
        self.get(dispute_id).await?.ok_or(Error::NotFound)
    }

    /// Reject a dispute with a plain-language reason + appeals-path explanation
    /// (AC-1). Works for any dispute type. Logs the decision.
    pub async fn reject(
        &self,
        dispute_id: uuid::Uuid,
        operator: &str,
        grounds: &str,
    ) -> Result<DisputeRecord, Error> {
        let mut tx = self.pool.begin().await?;

        let affected = sqlx::query(
            r#"
            UPDATE disputes
            SET status = 'rejected', resolved_by = $2, resolved_at = now(),
                resolution_grounds = $3, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(dispute_id)
        .bind(operator)
        .bind(grounds)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if affected == 0 {
            return Err(Error::NotFound);
        }

        self.insert_event(
            &mut tx,
            dispute_id,
            "rejected",
            operator,
            Some(grounds),
            serde_json::json!({ "appeals_path": "reply to the decision notification to appeal" }),
        )
        .await?;

        tx.commit().await?;
        self.get(dispute_id).await?.ok_or(Error::NotFound)
    }

    // -------------------------------------------------------------------------
    // Claim-conflict adjudication (AC-2) — DNS-TXT proof is the sole arbiter
    // -------------------------------------------------------------------------

    /// Adjudicate a claim conflict. The `winner_email` is the party that proved
    /// DNS-TXT control; `loser_email` is the party to notify with the outcome
    /// and a re-claim option. Marks the entity `verified` (DNS-TXT) for the
    /// winner, marks the dispute resolved, and logs the adjudication + the
    /// losing-party notification.
    pub async fn adjudicate_claim_conflict(
        &self,
        dispute_id: uuid::Uuid,
        operator: &str,
        winner_email: &str,
        loser_email: Option<&str>,
        rationale: &str,
    ) -> Result<DisputeRecord, Error> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(
            r#"SELECT domain FROM disputes WHERE id = $1 AND dispute_type = 'claim_conflict'"#,
        )
        .bind(dispute_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or(Error::NotFound)?;
        let domain: String = row.get("domain");

        // Domain control is the arbiter: the winner's claim becomes verified via
        // DNS-TXT. (The proof itself was established by the 43.2 flow.)
        sqlx::query(
            r#"
            UPDATE entities
            SET claim_status = 'verified', verification_method = 'dns_txt',
                verified_at = now(), updated_at = now()
            WHERE domain = $1
            "#,
        )
        .bind(&domain)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE disputes
            SET status = 'resolved', resolved_by = $2, resolved_at = now(),
                resolution_grounds = $3, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(dispute_id)
        .bind(operator)
        .bind(rationale)
        .execute(&mut *tx)
        .await?;

        self.insert_event(
            &mut tx,
            dispute_id,
            "conflict_adjudicated",
            operator,
            Some(rationale),
            serde_json::json!({
                "arbiter": "dns_txt_control",
                "winner_email": winner_email,
                "loser_email": loser_email,
            }),
        )
        .await?;

        // Losing-party notification record (AC-2): outcome + re-claim option.
        if let Some(loser) = loser_email {
            self.insert_event(
                &mut tx,
                dispute_id,
                "notification_sent",
                "system",
                Some("losing party notified: outcome + re-claim option"),
                serde_json::json!({
                    "recipient": loser,
                    "outcome": "lost",
                    "reclaim_available": true,
                }),
            )
            .await?;
        }

        tx.commit().await?;
        self.get(dispute_id).await?.ok_or(Error::NotFound)
    }

    // -------------------------------------------------------------------------
    // Change-of-control (domain ownership transfer)
    // -------------------------------------------------------------------------

    /// Resolve a change-of-control request. Domain control is the arbiter: the
    /// new owner (who re-proved DNS-TXT) takes over the verified claim. Records
    /// the transfer in the audit log.
    pub async fn transfer_control(
        &self,
        dispute_id: uuid::Uuid,
        operator: &str,
        new_owner_email: &str,
        rationale: &str,
    ) -> Result<DisputeRecord, Error> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(
            r#"SELECT domain FROM disputes WHERE id = $1 AND dispute_type = 'change_of_control'"#,
        )
        .bind(dispute_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or(Error::NotFound)?;
        let domain: String = row.get("domain");

        sqlx::query(
            r#"
            UPDATE entities
            SET claim_status = 'verified', verification_method = 'dns_txt',
                verified_at = now(), grace_period_start = NULL, updated_at = now()
            WHERE domain = $1
            "#,
        )
        .bind(&domain)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE disputes
            SET status = 'resolved', resolved_by = $2, resolved_at = now(),
                resolution_grounds = $3, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(dispute_id)
        .bind(operator)
        .bind(rationale)
        .execute(&mut *tx)
        .await?;

        self.insert_event(
            &mut tx,
            dispute_id,
            "control_transferred",
            operator,
            Some(rationale),
            serde_json::json!({
                "arbiter": "dns_txt_control",
                "new_owner_email": new_owner_email,
            }),
        )
        .await?;

        tx.commit().await?;
        self.get(dispute_id).await?.ok_or(Error::NotFound)
    }

    // -------------------------------------------------------------------------
    // GDPR Art.21 objection assessment (AC-3)
    // -------------------------------------------------------------------------

    /// Record a case-by-case GDPR Art.21 assessment. `honored` reflects whether
    /// the data is genuinely personal and no compelling legitimate grounds
    /// override. When honored, processing stops: the domain is suppressed from
    /// public display. The outcome + grounds are stored in the audit log.
    ///
    /// The 30-day window (AC-3) is an operational SLA enforced by the surface;
    /// this records the assessment result.
    pub async fn assess_gdpr_objection(
        &self,
        dispute_id: uuid::Uuid,
        operator: &str,
        honored: bool,
        grounds: &str,
    ) -> Result<DisputeRecord, Error> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query(
            r#"SELECT domain FROM disputes WHERE id = $1 AND dispute_type = 'gdpr_objection'"#,
        )
        .bind(dispute_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or(Error::NotFound)?;
        let domain: String = row.get("domain");

        let (status, new_suppressed) = if honored {
            // Processing stops: suppress from public display.
            sqlx::query(r#"UPDATE disputes SET suppressed = TRUE WHERE id = $1"#)
                .bind(dispute_id)
                .execute(&mut *tx)
                .await?;
            ("approved", true)
        } else {
            ("rejected", false)
        };

        sqlx::query(
            r#"
            UPDATE disputes
            SET status = $2, resolved_by = $3, resolved_at = now(),
                resolution_grounds = $4, updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(dispute_id)
        .bind(status)
        .bind(operator)
        .bind(grounds)
        .execute(&mut *tx)
        .await?;

        self.insert_event(
            &mut tx,
            dispute_id,
            "gdpr_assessed",
            operator,
            Some(grounds),
            serde_json::json!({
                "article": "GDPR Art.21",
                "outcome": if honored { "honored" } else { "refused" },
                "domain": domain,
                "processing_stopped": new_suppressed,
            }),
        )
        .await?;

        tx.commit().await?;
        self.get(dispute_id).await?.ok_or(Error::NotFound)
    }

    // -------------------------------------------------------------------------
    // Suppression helpers (AC-4)
    // -------------------------------------------------------------------------

    /// Whether the given (normalized) domain has any active suppression.
    pub async fn is_suppressed(&self, domain: &str) -> Result<bool, Error> {
        let row = sqlx::query(
            r#"SELECT EXISTS(SELECT 1 FROM disputes WHERE domain = $1 AND suppressed = TRUE) AS s"#,
        )
        .bind(domain)
        .fetch_one(self.pool)
        .await?;
        Ok(row.get::<bool, _>("s"))
    }

    /// Record that a notification was sent for a dispute (audit only).
    pub async fn record_notification(
        &self,
        dispute_id: uuid::Uuid,
        recipient: &str,
        summary: &str,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            INSERT INTO dispute_events (dispute_id, event_type, actor, rationale, detail)
            VALUES ($1, 'notification_sent', 'system', $2, $3)
            "#,
        )
        .bind(dispute_id)
        .bind(summary)
        .bind(serde_json::json!({ "recipient": recipient }))
        .execute(self.pool)
        .await?;
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Internal
    // -------------------------------------------------------------------------

    async fn insert_event(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        dispute_id: uuid::Uuid,
        event_type: &str,
        actor: &str,
        rationale: Option<&str>,
        detail: serde_json::Value,
    ) -> Result<(), Error> {
        sqlx::query(
            r#"
            INSERT INTO dispute_events (dispute_id, event_type, actor, rationale, detail)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(dispute_id)
        .bind(event_type)
        .bind(actor)
        .bind(rationale)
        .bind(detail)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}
