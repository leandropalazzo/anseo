//! Dynamic-`sqlx` access to the `comms_*` tables.
//!
//! Per the story's storage discipline, this uses runtime `sqlx::query` /
//! `sqlx::query_as::<_, Row>` with `#[derive(sqlx::FromRow)]` — never the
//! compile-time `query!` macros.
//!
//! Three concerns:
//!   * **subscriptions** — the recipient's marketing preference state +
//!     EU-consent flags (the preference center reads/writes these).
//!   * **suppression** — the hard suppression list, consulted before every
//!     marketing send.
//!   * **send log** — append-only audit (recipient hashed) + the magic-link
//!     idempotency guard.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use sqlx::Row as _;

/// The recipient's marketing preference center state.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Subscription {
    pub recipient_hash: String,
    pub email: String,
    pub rank_change_enabled: bool,
    pub digest_frequency: String,
    pub all_marketing_off: bool,
    pub marketing_consent: bool,
    pub is_eu_resident: bool,
}

/// A toggle payload from the preference center (AC-3). `None` fields are left
/// unchanged.
#[derive(Debug, Clone, Default)]
pub struct PreferenceUpdate {
    pub rank_change_enabled: Option<bool>,
    pub digest_frequency: Option<String>,
    pub all_marketing_off: Option<bool>,
    pub marketing_consent: Option<bool>,
}

/// Reason a recipient is suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionReason {
    Unsubscribe,
    Bounce,
    Complaint,
    /// GDPR Art.21(2) objection — immediate + absolute.
    GdprObjection,
}

impl SuppressionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SuppressionReason::Unsubscribe => "unsubscribe",
            SuppressionReason::Bounce => "bounce",
            SuppressionReason::Complaint => "complaint",
            SuppressionReason::GdprObjection => "gdpr_objection",
        }
    }
}

/// Audit-log outcome of a send attempt (AC-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    Failed,
    Suppressed,
    ConsentBlocked,
}

impl SendOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            SendOutcome::Sent => "sent",
            SendOutcome::Failed => "failed",
            SendOutcome::Suppressed => "suppressed",
            SendOutcome::ConsentBlocked => "consent_blocked",
        }
    }
}

/// Repository over the `comms_*` tables.
pub struct CommsRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> CommsRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    // -------------------------------------------------------------------------
    // Subscriptions
    // -------------------------------------------------------------------------

    /// Upsert a subscription, returning the row. Used when a recipient first
    /// opts in or when the preference center first materialises their record.
    pub async fn upsert_subscription(
        &self,
        recipient_hash: &str,
        email: &str,
        is_eu_resident: bool,
    ) -> Result<Subscription, sqlx::Error> {
        let row = sqlx::query_as::<_, Subscription>(
            r#"
            INSERT INTO comms_subscriptions (recipient_hash, email, is_eu_resident)
            VALUES ($1, $2, $3)
            ON CONFLICT (recipient_hash) DO UPDATE
                SET email = EXCLUDED.email,
                    updated_at = now()
            RETURNING recipient_hash, email, rank_change_enabled, digest_frequency,
                      all_marketing_off, marketing_consent, is_eu_resident
            "#,
        )
        .bind(recipient_hash)
        .bind(email)
        .bind(is_eu_resident)
        .fetch_one(self.pool)
        .await?;
        Ok(row)
    }

    /// Look up a subscription by recipient hash.
    pub async fn get_subscription(
        &self,
        recipient_hash: &str,
    ) -> Result<Option<Subscription>, sqlx::Error> {
        sqlx::query_as::<_, Subscription>(
            r#"
            SELECT recipient_hash, email, rank_change_enabled, digest_frequency,
                   all_marketing_off, marketing_consent, is_eu_resident
            FROM comms_subscriptions
            WHERE recipient_hash = $1
            "#,
        )
        .bind(recipient_hash)
        .fetch_optional(self.pool)
        .await
    }

    /// Apply a granular preference update (AC-3). COALESCE keeps unspecified
    /// fields unchanged.
    pub async fn update_preferences(
        &self,
        recipient_hash: &str,
        update: &PreferenceUpdate,
    ) -> Result<Option<Subscription>, sqlx::Error> {
        sqlx::query_as::<_, Subscription>(
            r#"
            UPDATE comms_subscriptions
            SET rank_change_enabled = COALESCE($2, rank_change_enabled),
                digest_frequency    = COALESCE($3, digest_frequency),
                all_marketing_off   = COALESCE($4, all_marketing_off),
                marketing_consent   = COALESCE($5, marketing_consent),
                updated_at          = now()
            WHERE recipient_hash = $1
            RETURNING recipient_hash, email, rank_change_enabled, digest_frequency,
                      all_marketing_off, marketing_consent, is_eu_resident
            "#,
        )
        .bind(recipient_hash)
        .bind(update.rank_change_enabled)
        .bind(update.digest_frequency.as_deref())
        .bind(update.all_marketing_off)
        .bind(update.marketing_consent)
        .fetch_optional(self.pool)
        .await
    }

    // -------------------------------------------------------------------------
    // Suppression
    // -------------------------------------------------------------------------

    /// Add a recipient to the suppression list. Idempotent: a repeated
    /// suppression updates the reason/scope/time.
    pub async fn suppress(
        &self,
        recipient_hash: &str,
        reason: SuppressionReason,
        scope: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO comms_suppressions (recipient_hash, reason, scope)
            VALUES ($1, $2, $3)
            ON CONFLICT (recipient_hash) DO UPDATE
                SET reason = EXCLUDED.reason,
                    scope = EXCLUDED.scope,
                    suppressed_at = now()
            "#,
        )
        .bind(recipient_hash)
        .bind(reason.as_str())
        .bind(scope)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// True if the recipient is suppressed for marketing (scope 'marketing' or
    /// 'all'). Transactional mail ignores marketing-scope suppression.
    pub async fn is_marketing_suppressed(&self, recipient_hash: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 1 AS hit
            FROM comms_suppressions
            WHERE recipient_hash = $1 AND scope IN ('marketing', 'all')
            "#,
        )
        .bind(recipient_hash)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.is_some())
    }

    // -------------------------------------------------------------------------
    // Preference tokens
    // -------------------------------------------------------------------------

    /// Store a minted token's hash bound to a recipient.
    pub async fn store_token(
        &self,
        token_hash: &str,
        recipient_hash: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO comms_preference_tokens (token_hash, recipient_hash, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (token_hash) DO NOTHING
            "#,
        )
        .bind(token_hash)
        .bind(recipient_hash)
        .bind(expires_at)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// Resolve a token hash to its recipient hash, honouring expiry. Returns
    /// `None` if unknown or expired.
    pub async fn resolve_token(&self, token_hash: &str) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT recipient_hash
            FROM comms_preference_tokens
            WHERE token_hash = $1
              AND (expires_at IS NULL OR expires_at > now())
            "#,
        )
        .bind(token_hash)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.map(|r| r.get::<String, _>("recipient_hash")))
    }

    // -------------------------------------------------------------------------
    // Send log (audit + idempotency)
    // -------------------------------------------------------------------------

    /// Append a send-log row (AC-5). `recipient_hash` is hashed; `error` is
    /// populated on failure. `dedup_key` set for magic-link idempotency.
    #[allow(clippy::too_many_arguments)]
    pub async fn log_send(
        &self,
        recipient_hash: &str,
        stream: &str,
        email_type: &str,
        outcome: SendOutcome,
        error: Option<&str>,
        dedup_key: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO comms_send_log
                (recipient_hash, stream, email_type, outcome, error, dedup_key)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(recipient_hash)
        .bind(stream)
        .bind(email_type)
        .bind(outcome.as_str())
        .bind(error)
        .bind(dedup_key)
        .execute(self.pool)
        .await?;
        Ok(())
    }

    /// True if a magic-link send for this dedup key already succeeded (AC-5:
    /// no retry within the expiry window).
    pub async fn already_sent_dedup(&self, dedup_key: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT 1 AS hit
            FROM comms_send_log
            WHERE dedup_key = $1 AND outcome = 'sent'
            "#,
        )
        .bind(dedup_key)
        .fetch_optional(self.pool)
        .await?;
        Ok(row.is_some())
    }
}
