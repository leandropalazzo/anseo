//! Story 13.1 — consent + audit storage for the public benchmark dataset.
//!
//! Two operations: `record_optin` and `record_optout`. Both append a row
//! to `benchmark_consent`; the redactor reads the most-recent row to
//! decide whether the current operator is on the current
//! `TERMS_VERSION`.

use anseo_core::ids::ProjectId;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::Error;

/// Consent tier. The anonymous-aggregate tier (Story 13.1) and the
/// brand-visibility identified tier (Story 44.1) are recorded and revoked
/// **independently** — a project may be anonymously opted in while remaining
/// identified-out, and vice versa. APPEARING ≠ CLAIMING: only the
/// `BrandVisibility` tier authorizes transmitting brand identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentTier {
    Anonymous,
    BrandVisibility,
}

impl ConsentTier {
    /// The string stored in the `tier` column (must match the migration CHECK).
    pub fn as_str(self) -> &'static str {
        match self {
            ConsentTier::Anonymous => "anonymous",
            ConsentTier::BrandVisibility => "brand_visibility",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConsentRow {
    pub id: Uuid,
    pub project_id: ProjectId,
    pub event: String,
    pub tier: String,
    pub terms_version: String,
    pub actor: Option<String>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ConsentRow {
    /// True iff this row represents an *active* consent for the current terms:
    /// the most-recent event is `optin` and the pinned terms version matches.
    /// Caller is responsible for having selected the latest row in the relevant
    /// tier.
    pub fn is_active(&self, current_terms: &str) -> bool {
        self.event == "optin" && self.terms_version == current_terms
    }
}

pub struct BenchmarkConsentRepo<'a> {
    pool: &'a PgPool,
}

impl<'a> BenchmarkConsentRepo<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Append an opt-in event on the **anonymous** tier (Story 13.1
    /// back-compat). Equivalent to
    /// `record_optin_tier(.., ConsentTier::Anonymous, ..)`.
    pub async fn record_optin(
        &self,
        project_id: ProjectId,
        terms_version: &str,
        actor: Option<&str>,
        note: Option<&str>,
    ) -> Result<Uuid, Error> {
        self.record_event(
            project_id,
            "optin",
            ConsentTier::Anonymous,
            terms_version,
            actor,
            note,
        )
        .await
    }

    /// Append an opt-out event on the **anonymous** tier (Story 13.1
    /// back-compat).
    pub async fn record_optout(
        &self,
        project_id: ProjectId,
        terms_version: &str,
        actor: Option<&str>,
        note: Option<&str>,
    ) -> Result<Uuid, Error> {
        self.record_event(
            project_id,
            "optout",
            ConsentTier::Anonymous,
            terms_version,
            actor,
            note,
        )
        .await
    }

    /// Append an opt-in event on `tier` (Story 44.1). The brand-visibility
    /// tier is the EXPLICIT identified opt-in; it is recorded independently of
    /// the anonymous tier. Returns the new consent record id — for the
    /// identified tier this is the id every identified contribution must
    /// reference (CC-NFR2 provenance, see `contributions.consent_record_id`).
    pub async fn record_optin_tier(
        &self,
        project_id: ProjectId,
        tier: ConsentTier,
        terms_version: &str,
        actor: Option<&str>,
        note: Option<&str>,
    ) -> Result<Uuid, Error> {
        self.record_event(project_id, "optin", tier, terms_version, actor, note)
            .await
    }

    /// Append an opt-out event on `tier` (Story 44.1). Withdrawal is one action
    /// (GDPR Art.7(3)): a single append flips the tier inactive immediately.
    pub async fn record_optout_tier(
        &self,
        project_id: ProjectId,
        tier: ConsentTier,
        terms_version: &str,
        actor: Option<&str>,
        note: Option<&str>,
    ) -> Result<Uuid, Error> {
        self.record_event(project_id, "optout", tier, terms_version, actor, note)
            .await
    }

    /// Shared append. CC-NFR2 append-only: every consent change is a new row,
    /// never an update or delete.
    async fn record_event(
        &self,
        project_id: ProjectId,
        event: &str,
        tier: ConsentTier,
        terms_version: &str,
        actor: Option<&str>,
        note: Option<&str>,
    ) -> Result<Uuid, Error> {
        let id = Uuid::from_u128(ulid::Ulid::new().0);
        sqlx::query(
            r#"INSERT INTO benchmark_consent
               (id, project_id, event, tier, terms_version, actor, note)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(id)
        .bind(project_id)
        .bind(event)
        .bind(tier.as_str())
        .bind(terms_version)
        .bind(actor)
        .bind(note)
        .execute(self.pool)
        .await?;
        Ok(id)
    }

    /// Most-recent consent row for this project on the **anonymous** tier, or
    /// `None` if the project has never opted in to it. Back-compat shim over
    /// [`Self::latest_for_tier`] preserving the Story 13.1 signature.
    pub async fn latest_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<ConsentRow>, Error> {
        self.latest_for_tier(project_id, ConsentTier::Anonymous)
            .await
    }

    /// Most-recent consent row for this project on `tier`, or `None` if the
    /// project has never opted in to that tier. The caller decides activeness
    /// via [`ConsentRow::is_active`] (most recent event is `optin` AND the
    /// pinned terms version matches the current terms).
    pub async fn latest_for_tier(
        &self,
        project_id: ProjectId,
        tier: ConsentTier,
    ) -> Result<Option<ConsentRow>, Error> {
        let row = sqlx::query(
            r#"SELECT id, project_id, event, tier, terms_version, actor, note, created_at
               FROM benchmark_consent
               WHERE project_id = $1 AND tier = $2
               ORDER BY created_at DESC
               LIMIT 1"#,
        )
        .bind(project_id)
        .bind(tier.as_str())
        .fetch_optional(self.pool)
        .await?;
        row.map(map_consent_row).transpose()
    }

    // ─────────────────────────────────────────────────────────────────────
    // Story 49.0 — operator-admin Plane-1 READ-ONLY surface over the OSS-owned
    // `benchmark_consent` ledger. No mutation path is exposed here.
    // ─────────────────────────────────────────────────────────────────────

    /// List consent records filtered by tier / project / event / time range,
    /// newest-first, paginated. Read-only (the operator consent-records read,
    /// 49.0). All filters are optional; an absent filter matches all rows.
    pub async fn list_records(&self, f: &ConsentReadFilters) -> Result<Vec<ConsentRow>, Error> {
        let rows = sqlx::query(
            r#"SELECT id, project_id, event, tier, terms_version, actor, note, created_at
               FROM benchmark_consent
               WHERE ($1::text IS NULL OR tier = $1)
                 AND ($2::uuid IS NULL OR project_id = $2)
                 AND ($3::text IS NULL OR event = $3)
                 AND ($4::timestamptz IS NULL OR created_at >= $4)
                 AND ($5::timestamptz IS NULL OR created_at <= $5)
               ORDER BY created_at DESC, id DESC
               LIMIT $6 OFFSET $7"#,
        )
        .bind(f.tier.as_ref().map(|t| t.as_str()))
        .bind(f.project_id)
        .bind(f.event.as_deref())
        .bind(f.from)
        .bind(f.to)
        .bind(f.limit)
        .bind(f.offset)
        .fetch_all(self.pool)
        .await?;
        rows.into_iter().map(map_consent_row).collect()
    }

    /// Distinct project ids that have at least one consent record. Backs the
    /// per-project kek-status read (49.0): the set of projects whose KEK status
    /// the operator can ask about, derived purely from OSS-owned consent data.
    pub async fn distinct_projects(&self) -> Result<Vec<ProjectId>, Error> {
        let rows = sqlx::query(
            r#"SELECT DISTINCT project_id FROM benchmark_consent ORDER BY project_id"#,
        )
        .fetch_all(self.pool)
        .await?;
        rows.into_iter()
            .map(|r| r.try_get::<ProjectId, _>("project_id").map_err(Error::from))
            .collect()
    }
}

/// Filters for the operator consent-records read (49.0). All optional; an
/// absent filter matches all rows. `limit`/`offset` are clamped by the caller.
#[derive(Debug, Clone, Default)]
pub struct ConsentReadFilters {
    pub tier: Option<ConsentTier>,
    pub project_id: Option<ProjectId>,
    /// `optin` | `optout`.
    pub event: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
}

fn map_consent_row(r: sqlx::postgres::PgRow) -> Result<ConsentRow, Error> {
    Ok(ConsentRow {
        id: r.try_get("id")?,
        project_id: r.try_get("project_id")?,
        event: r.try_get("event")?,
        tier: r.try_get("tier")?,
        terms_version: r.try_get("terms_version")?,
        actor: r.try_get("actor")?,
        note: r.try_get("note")?,
        created_at: r.try_get("created_at")?,
    })
}
