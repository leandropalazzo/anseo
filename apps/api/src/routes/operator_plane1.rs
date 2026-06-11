//! Plane-1 OSS operator substrate — Story 49.0.
//!
//! Seven operator-admin-scoped endpoints the admin console (49.1 / 49.4 / 49.7)
//! brokers. They expose ONLY generic operator capabilities over data the OSS
//! repo already owns (ADR-007: no admin-console-specific logic in OSS). All are
//! gated by the SAME `require_operator_key` / `ANSEO_OPERATOR_API_KEY` mechanism
//! as the 48.4 operator-entity surface (401 missing / 403 wrong key / 503
//! unconfigured) — mounted under that gate in `lib.rs`.
//!
//! Consent / contribution READS (D1) — read-only, over OSS-owned durable data:
//!   * `GET /operator/consent/records`          — `benchmark_consent` ledger.
//!   * `GET /operator/consent/events`           — opt-in/out event stream.
//!   * `GET /operator/consent/kek-status`       — per-project crypto-shred/KEK
//!     status; NEVER returns key material.
//!   * `GET /operator/contributions/density`    — per (provider × category ×
//!     window) counts feeding the k>=5 floor — parity with the density-floor
//!     source of truth (`density_check`, `contributor_count >= k`).
//!   * `GET /operator/verification/throughput`  — recent verify completions /
//!     failures over the 48.4 `verification_attempts` substrate.
//!
//! Terms-finalize GATE (D2) — OSS-owned source of truth in `crates/storage`:
//!   * `GET  /operator/config/benchmark-gate`   — readable by an OSS consumer
//!     (CLI optin / ingest) WITHOUT reading `anseo_admin`.
//!   * `PUT  /operator/config/benchmark-gate`   — operator-admin write; the
//!     source of truth lives here; a subsequent GET reflects it.
//!
//! benchmark-service stays UNTOUCHED. Dynamic sqlx only (no `query!` macros).

use anseo_benchmark::{kek_secret_key, ProjectKek};
use anseo_core::ids::ProjectId;
use anseo_storage::repositories::benchmark_consent::{ConsentReadFilters, ConsentRow, ConsentTier};
use anseo_storage::repositories::benchmark_gate::GateConfig;
use axum::extract::{Extension, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::AppState;

type ApiError = (StatusCode, Json<serde_json::Value>);

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
/// Default rolling window for the density / throughput reads (days).
const DEFAULT_WINDOW_DAYS: i64 = 30;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/operator/consent/records", get(consent_records))
        .route("/operator/consent/events", get(consent_events))
        .route("/operator/consent/kek-status", get(kek_status))
        .route(
            "/operator/contributions/density",
            get(contributions_density),
        )
        .route(
            "/operator/verification/throughput",
            get(verification_throughput),
        )
        .route(
            "/operator/config/benchmark-gate",
            get(get_benchmark_gate).put(put_benchmark_gate),
        )
}

fn storage_err(e: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
    )
}

fn bad_request(code: &str, msg: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": code, "message": msg })),
    )
}

fn clamp_limit(requested: Option<i64>) -> i64 {
    requested.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn clamp_offset(requested: Option<i64>) -> i64 {
    requested.unwrap_or(0).max(0)
}

/// Parse the optional `tier` filter into a [`ConsentTier`]. Returns the parsed
/// tier (or `None` to mean "all tiers"); an unrecognised value is a 400.
fn parse_tier(raw: Option<&str>) -> Result<Option<ConsentTier>, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some("anonymous") => Ok(Some(ConsentTier::Anonymous)),
        Some("brand_visibility") => Ok(Some(ConsentTier::BrandVisibility)),
        Some(other) => Err(bad_request(
            "invalid_tier",
            &format!("unknown tier '{other}'; expected 'anonymous' or 'brand_visibility'"),
        )),
    }
}

fn parse_event(raw: Option<&str>) -> Result<Option<String>, ApiError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(e @ ("optin" | "optout")) => Ok(Some(e.to_string())),
        Some(other) => Err(bad_request(
            "invalid_event",
            &format!("unknown event '{other}'; expected 'optin' or 'optout'"),
        )),
    }
}

fn parse_project(raw: Option<&str>) -> Result<Option<ProjectId>, ApiError> {
    use std::str::FromStr;
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(p) => ProjectId::from_str(p)
            .map(Some)
            .map_err(|_| bad_request("invalid_project", "project must be a valid project id")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Consent views
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ConsentRecordView {
    pub id: uuid::Uuid,
    pub project_id: String,
    pub event: String,
    pub tier: String,
    pub terms_version: String,
    pub actor: Option<String>,
    pub note: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<ConsentRow> for ConsentRecordView {
    fn from(r: ConsentRow) -> Self {
        Self {
            id: r.id,
            project_id: r.project_id.to_string(),
            event: r.event,
            tier: r.tier,
            terms_version: r.terms_version,
            actor: r.actor,
            note: r.note,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ConsentRecordsQuery {
    pub tier: Option<String>,
    pub project: Option<String>,
    pub event: Option<String>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ConsentRecordsResponse {
    pub records: Vec<ConsentRecordView>,
    pub limit: i64,
    pub offset: i64,
    pub count: usize,
}

fn build_filters(q: &ConsentRecordsQuery) -> Result<ConsentReadFilters, ApiError> {
    Ok(ConsentReadFilters {
        tier: parse_tier(q.tier.as_deref())?,
        project_id: parse_project(q.project.as_deref())?,
        event: parse_event(q.event.as_deref())?,
        from: q.from,
        to: q.to,
        limit: clamp_limit(q.limit),
        offset: clamp_offset(q.offset),
    })
}

async fn consent_records(
    State(state): State<AppState>,
    Query(q): Query<ConsentRecordsQuery>,
) -> Result<Json<ConsentRecordsResponse>, ApiError> {
    let filters = build_filters(&q)?;
    let rows = state
        .storage
        .benchmark_consent()
        .list_records(&filters)
        .await
        .map_err(storage_err)?;
    let records: Vec<ConsentRecordView> = rows.into_iter().map(ConsentRecordView::from).collect();
    Ok(Json(ConsentRecordsResponse {
        count: records.len(),
        records,
        limit: filters.limit,
        offset: filters.offset,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /operator/consent/events — opt-in/opt-out stream (terms_version + ts)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ConsentEventView {
    pub id: uuid::Uuid,
    pub project_id: String,
    pub event: String,
    pub tier: String,
    pub terms_version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct ConsentEventsResponse {
    pub events: Vec<ConsentEventView>,
    pub limit: i64,
    pub offset: i64,
    pub count: usize,
}

/// The opt-in/out event stream is the SAME `benchmark_consent` ledger projected
/// to the event-relevant columns (event + terms_version + timestamp). Read-only.
async fn consent_events(
    State(state): State<AppState>,
    Query(q): Query<ConsentRecordsQuery>,
) -> Result<Json<ConsentEventsResponse>, ApiError> {
    let filters = build_filters(&q)?;
    let rows = state
        .storage
        .benchmark_consent()
        .list_records(&filters)
        .await
        .map_err(storage_err)?;
    let events: Vec<ConsentEventView> = rows
        .into_iter()
        .map(|r| ConsentEventView {
            id: r.id,
            project_id: r.project_id.to_string(),
            event: r.event,
            tier: r.tier,
            terms_version: r.terms_version,
            created_at: r.created_at,
        })
        .collect();
    Ok(Json(ConsentEventsResponse {
        count: events.len(),
        events,
        limit: filters.limit,
        offset: filters.offset,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /operator/consent/kek-status — per-project crypto-shred/KEK status
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct KekStatusView {
    pub project_id: String,
    /// `active` (a KEK is loadable), `shredded` (project has identified
    /// contributions but no KEK — crypto-shredded), or `pending` (consented but
    /// no KEK provisioned yet). NEVER carries key material.
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct KekStatusResponse {
    pub projects: Vec<KekStatusView>,
    pub count: usize,
}

/// The KEK status derivation (pure; unit-tested). Decides the status string
/// from ONLY non-secret signals: whether a KEK is currently loadable for the
/// project, and whether the project has identified contributions on record.
///
/// SAFETY: this function never sees, returns, or logs key material — it takes
/// two booleans. The secret store distinguishes a present KEK from an absent
/// one but NOT "shredded" from "never existed"; we disambiguate using the
/// OSS-owned identified-contribution count: a project that contributed
/// identified data but has no KEK has been crypto-shredded.
pub fn derive_kek_status(kek_present: bool, has_identified_contributions: bool) -> &'static str {
    match (kek_present, has_identified_contributions) {
        (true, _) => "active",
        (false, true) => "shredded",
        (false, false) => "pending",
    }
}

async fn kek_status(State(state): State<AppState>) -> Result<Json<KekStatusResponse>, ApiError> {
    // The set of projects to report on comes from OSS-owned consent data.
    let projects = state
        .storage
        .benchmark_consent()
        .distinct_projects()
        .await
        .map_err(storage_err)?;

    let store = anseo_core::default_chain();
    let mut out = Vec::with_capacity(projects.len());
    for pid in projects {
        let pid_str = pid.to_string();
        // Presence check ONLY — we never construct/return the KEK value.
        let kek_present = ProjectKek::load(&store, &pid_str).is_ok();
        let _ = kek_secret_key(&pid_str); // wire-shape reference; no value read.
        let has_contribs: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)::bigint FROM contributions WHERE project_id = $1"#,
        )
        .bind(pid)
        .fetch_one(state.storage.pool())
        .await
        .map_err(storage_err)?;
        out.push(KekStatusView {
            project_id: pid_str.clone(),
            status: derive_kek_status(kek_present, has_contribs > 0).to_string(),
        });
    }
    Ok(Json(KekStatusResponse {
        count: out.len(),
        projects: out,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /operator/contributions/density — per (provider × category × window)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DensityQuery {
    /// Rolling window in days (defaults to 30 — the floor's window).
    pub window_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DensitySegmentView {
    pub provider: String,
    pub category: String,
    pub window_days: i64,
    pub contributor_count: i64,
    /// Whether this segment meets the k>=`density_floor` floor (parity with the
    /// density-floor source of truth used by `density_check`).
    pub meets_floor: bool,
}

#[derive(Debug, Serialize)]
pub struct DensityResponse {
    /// The active k>=N density floor (from the OSS-owned gate config).
    pub density_floor: i64,
    pub window_days: i64,
    pub segments: Vec<DensitySegmentView>,
    pub count: usize,
}

async fn contributions_density(
    State(state): State<AppState>,
    Query(q): Query<DensityQuery>,
) -> Result<Json<DensityResponse>, ApiError> {
    let window_days = q.window_days.unwrap_or(DEFAULT_WINDOW_DAYS).max(1);
    // The density floor comes from the OSS-owned gate config (source of truth).
    let gate = state
        .storage
        .benchmark_gate()
        .get()
        .await
        .map_err(storage_err)?;
    let floor = gate.density_floor as i64;

    // Parity with the density-floor source of truth (`density_check`): the SAME
    // `benchmark_segment_stats` table and the SAME `contributor_count >= floor`
    // predicate, here surfaced per (provider × category × window) rather than
    // collapsed to a category count. The table is externally populated (ETL /
    // benchmark display) and may be absent on a fresh deployment — tolerate that
    // with an empty result rather than erroring (matches density_check's
    // unwrap_or(0) posture).
    let rows = sqlx::query(
        r#"
        SELECT provider, category, window_days, contributor_count
        FROM benchmark_segment_stats
        WHERE window_days = $1
        ORDER BY provider, category
        "#,
    )
    .bind(window_days)
    .fetch_all(state.storage.pool())
    .await
    .unwrap_or_default();

    let segments: Vec<DensitySegmentView> = rows
        .into_iter()
        .map(|r| {
            let contributor_count: i64 = r.get("contributor_count");
            DensitySegmentView {
                provider: r.get("provider"),
                category: r.get("category"),
                window_days: r.get("window_days"),
                contributor_count,
                meets_floor: contributor_count >= floor,
            }
        })
        .collect();

    Ok(Json(DensityResponse {
        density_floor: floor,
        window_days,
        count: segments.len(),
        segments,
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /operator/verification/throughput — recent verify completions/failures
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ThroughputQuery {
    /// Look-back window in hours (defaults to 24).
    pub window_hours: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ThroughputResponse {
    pub window_hours: i64,
    pub verified: i64,
    pub failed: i64,
    pub revoked: i64,
    pub expired: i64,
    pub pending: i64,
    pub total: i64,
}

/// Reuses the 48.4 `verification_attempts` substrate (no new table). Counts
/// recent attempts by terminal status over the look-back window — the signal
/// 49.7 surfaces. Read-only.
async fn verification_throughput(
    State(state): State<AppState>,
    Query(q): Query<ThroughputQuery>,
) -> Result<Json<ThroughputResponse>, ApiError> {
    let window_hours = q.window_hours.unwrap_or(24).max(1);
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*) FILTER (WHERE status = 'verified')::bigint AS verified,
            COUNT(*) FILTER (WHERE status = 'failed')::bigint   AS failed,
            COUNT(*) FILTER (WHERE status = 'revoked')::bigint  AS revoked,
            COUNT(*) FILTER (WHERE status = 'expired')::bigint  AS expired,
            COUNT(*) FILTER (WHERE status = 'pending')::bigint  AS pending,
            COUNT(*)::bigint AS total
        FROM verification_attempts
        WHERE created_at >= now() - make_interval(hours => $1::int)
        "#,
    )
    .bind(window_hours)
    .fetch_one(state.storage.pool())
    .await
    .map_err(storage_err)?;

    Ok(Json(ThroughputResponse {
        window_hours,
        verified: row.get("verified"),
        failed: row.get("failed"),
        revoked: row.get("revoked"),
        expired: row.get("expired"),
        pending: row.get("pending"),
        total: row.get("total"),
    }))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET / PUT /operator/config/benchmark-gate — OSS-owned terms-finalize gate
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct GateView {
    pub terms_finalized: bool,
    pub terms_version: String,
    pub density_floor: i64,
    pub updated_by: Option<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<GateConfig> for GateView {
    fn from(g: GateConfig) -> Self {
        Self {
            terms_finalized: g.terms_finalized,
            terms_version: g.terms_version,
            density_floor: g.density_floor as i64,
            updated_by: g.updated_by,
            updated_at: g.updated_at,
        }
    }
}

async fn get_benchmark_gate(State(state): State<AppState>) -> Result<Json<GateView>, ApiError> {
    let gate = state
        .storage
        .benchmark_gate()
        .get()
        .await
        .map_err(storage_err)?;
    Ok(Json(GateView::from(gate)))
}

#[derive(Debug, Deserialize)]
pub struct GatePutBody {
    pub terms_finalized: bool,
    pub terms_version: String,
    pub density_floor: i64,
    #[serde(default)]
    pub operator: Option<String>,
}

async fn put_benchmark_gate(
    State(state): State<AppState>,
    Extension(op): Extension<crate::middleware::auth::AuthenticatedOperator>,
    Json(body): Json<GatePutBody>,
) -> Result<Json<GateView>, ApiError> {
    if body.terms_version.trim().is_empty() {
        return Err(bad_request(
            "terms_version_required",
            "terms_version must be a non-empty string",
        ));
    }
    if body.density_floor < 1 {
        return Err(bad_request(
            "invalid_density_floor",
            "density_floor must be >= 1",
        ));
    }
    let actor = body
        .operator
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| op.actor.clone());

    let gate = state
        .storage
        .benchmark_gate()
        .upsert(
            body.terms_finalized,
            body.terms_version.trim(),
            body.density_floor as i32,
            actor.as_deref(),
        )
        .await
        .map_err(storage_err)?;
    tracing::info!(
        event = "operator.benchmark_gate_updated",
        terms_finalized = body.terms_finalized,
        terms_version = %body.terms_version.trim(),
        density_floor = body.density_floor,
        actor = ?actor,
    );
    Ok(Json(GateView::from(gate)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_offset_clamping() {
        assert_eq!(clamp_limit(None), DEFAULT_LIMIT);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(10_000)), MAX_LIMIT);
        assert_eq!(clamp_offset(Some(-1)), 0);
    }

    #[test]
    fn tier_event_parse() {
        assert!(parse_tier(None).unwrap().is_none());
        assert_eq!(
            parse_tier(Some("anonymous")).unwrap(),
            Some(ConsentTier::Anonymous)
        );
        assert_eq!(
            parse_tier(Some("brand_visibility")).unwrap(),
            Some(ConsentTier::BrandVisibility)
        );
        assert!(parse_tier(Some("bogus")).is_err());
        assert_eq!(
            parse_event(Some("optin")).unwrap(),
            Some("optin".to_string())
        );
        assert!(parse_event(Some("nope")).is_err());
    }

    #[test]
    fn kek_status_derivation() {
        // A loadable KEK → active, regardless of contributions.
        assert_eq!(derive_kek_status(true, true), "active");
        assert_eq!(derive_kek_status(true, false), "active");
        // No KEK but identified contributions exist → crypto-shredded.
        assert_eq!(derive_kek_status(false, true), "shredded");
        // No KEK and no contributions → pending provisioning.
        assert_eq!(derive_kek_status(false, false), "pending");
    }
}
