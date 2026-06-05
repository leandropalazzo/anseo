//! Phase 3 Epic 19 — GEO Recommendation REST surface.
//!
//! Story 19.5 shipped `GET /v1/recommendations/metrics` (SM-14 adoption).
//! Story 19.6 adds the generate / list / detail / transition surface:
//!
//! - `POST /v1/recommendations/generate` — assemble an `EngineInput` from the
//!   project's live prompts / runs / citations, run the in-process engine, and
//!   persist the result. Returns **202 Accepted** + a `status_url` per the
//!   Phase 2 async-write pattern.
//! - `GET /v1/recommendations` — cursor-paginated active recommendations.
//! - `GET /v1/recommendations/:id` — one recommendation + full traceability.
//! - `PATCH /v1/recommendations/:id/state` — lifecycle transition (Story 19.4
//!   state machine). Illegal transitions map to **409**.
//!
//! The four `recommendation.{generated,surfaced,acted,measured}` lifecycle
//! events are emitted onto the broadcast channel **and** fanned out to webhook
//! deliveries, reusing the Phase 2 HMAC signer + retry ladder unchanged
//! (architecture §4.4).
//!
//! SM-14 deliberately *quarantines* plugin-emitted Kinds; see `sm14_metric`.

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use uuid::Uuid;

use anseo_recommendations::assembly::{self, ProjectFacts, PromptFacts, PromptRunFacts};
use anseo_recommendations::lifecycle::{self, State as LifecycleState};
use anseo_recommendations::{Engine, Recommendation};
use anseo_scheduler::events::{LifecycleEvent, RecommendationPayload};
use anseo_storage::repositories::recommendations::{NewRecommendation, RecommendationRow};

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

type ApiError = (StatusCode, Json<JsonValue>);

fn err(status: StatusCode, error: &str, message: &str) -> ApiError {
    (status, Json(json!({ "error": error, "message": message })))
}

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/recommendations/metrics", get(metrics_handler))
        .route("/recommendations/intelligence", get(intelligence_handler))
        .route("/recommendations/generate", post(generate_handler))
        .route("/recommendations", get(list_handler))
        .route("/recommendations/:id", get(detail_handler))
        .route("/recommendations/:id/state", patch(transition_handler))
}

// ---- SM-14 metric (Story 19.5) ------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sm14MetricResponse {
    pub numerator: i64,
    pub denominator: i64,
    pub rate: Option<f64>,
}

async fn metrics_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
) -> Result<Json<Sm14MetricResponse>, ApiError> {
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    let metric = state
        .storage
        .recommendations()
        .sm14_metric(project_uuid)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "recommendations/metrics", "fetch failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "SM-14 metric fetch failed",
            )
        })?;
    Ok(Json(Sm14MetricResponse {
        numerator: metric.numerator,
        denominator: metric.denominator,
        rate: metric.rate(),
    }))
}

// ---- intelligence: what works vs what doesn't ---------------------------

#[derive(Debug, Serialize)]
struct KindAdoptionItem {
    kind: String,
    surfaced: i64,
    acted: i64,
    dismissed: i64,
    /// acted / surfaced, None when nothing surfaced yet.
    adoption_rate: Option<f64>,
}

async fn intelligence_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
) -> Result<Json<JsonValue>, ApiError> {
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    let by_kind = state
        .storage
        .recommendations()
        .adoption_by_kind(project_uuid)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, route = "recommendations/intelligence", "fetch failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "adoption intelligence fetch failed",
            )
        })?;
    let items: Vec<KindAdoptionItem> = by_kind
        .into_iter()
        .map(|k| KindAdoptionItem {
            adoption_rate: if k.surfaced == 0 {
                None
            } else {
                Some(k.acted as f64 / k.surfaced as f64)
            },
            kind: k.kind,
            surfaced: k.surfaced,
            acted: k.acted,
            dismissed: k.dismissed,
        })
        .collect();
    Ok(Json(json!({ "by_kind": items })))
}

// ---- POST /generate (Story 19.6) ----------------------------------------

/// Default evaluation window for a generation run (architecture §2: 14 days).
const WINDOW_DAYS: i64 = 14;

#[derive(Debug, Clone, Serialize)]
pub struct GenerateAccepted {
    pub status: String,
    /// Recommendations the engine produced this run.
    pub generated_count: usize,
    /// Of those, how many were freshly persisted (dedup may drop some).
    pub inserted_count: usize,
    /// Where to read the resulting recommendations (Phase 2 async pattern).
    pub status_url: String,
}

async fn generate_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<GenerateAccepted>), ApiError> {
    let config = state.config.as_ref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "config_unavailable",
            "no opengeo.yaml loaded; cannot assemble recommendation inputs",
        )
    })?;

    let now = Utc::now();
    let window_start = now - Duration::days(WINDOW_DAYS);
    let project_ulid = project_id.into_ulid();

    // Assemble live facts from Postgres (prompts → runs → citations).
    let db_prompts = state
        .storage
        .prompts()
        .list_by_project(project_id)
        .await
        .map_err(internal("list prompts"))?;

    let mut prompt_facts = Vec::with_capacity(db_prompts.len());
    for prompt in &db_prompts {
        let runs = state
            .storage
            .prompt_runs()
            .list_by_prompt_since(prompt.id, window_start)
            .await
            .map_err(internal("list runs"))?;
        let mut run_facts = Vec::with_capacity(runs.len());
        for run in &runs {
            let citations = state
                .storage
                .citations()
                .list_by_run(run.id)
                .await
                .map_err(internal("list citations"))?;
            run_facts.push(PromptRunFacts {
                run_id: run.id.into_ulid(),
                citation_domains: citations.iter().map(|c| c.domain.clone()).collect(),
                citation_ids: citations.iter().map(|c| c.id.into_ulid()).collect(),
            });
        }
        prompt_facts.push(PromptFacts {
            prompt_id: prompt.id.into_ulid(),
            prompt: prompt.text.clone(),
            runs: run_facts,
        });
    }

    let facts = ProjectFacts {
        project_id: project_ulid,
        brand: config.brand.name.clone(),
        brand_etld1: String::new(),
        docs_etld1: None,
        competitors: config.competitors.iter().map(|c| c.name.clone()).collect(),
        enabled_providers: config
            .providers
            .iter()
            .map(|p| p.name.as_wire_str().into_owned())
            .collect(),
        benchmark_opted_in: false,
        prompts: prompt_facts,
        window: anseo_recommendations::wire::TimeWindow {
            start: window_start,
            end: now,
        },
        generated_at: now,
    };

    let input = assembly::assemble(facts);
    let recs = Engine::default().generate(&input);
    let generated_count = recs.len();

    let project_uuid = Uuid::from_bytes(project_ulid.to_bytes());
    let mut inserted_count = 0usize;
    for rec in &recs {
        let new_row = rec_to_new_row(rec, project_uuid);
        let inserted = state
            .storage
            .recommendations()
            .insert(new_row)
            .await
            .map_err(internal("insert recommendation"))?;
        if let Some(id) = inserted {
            inserted_count += 1;
            emit_recommendation_event(
                &state,
                project_uuid,
                id,
                rec.kind.as_str(),
                &rec.summary,
                "generated",
            )
            .await;
        }
    }

    tracing::info!(
        project = %project_uuid,
        generated = generated_count,
        inserted = inserted_count,
        "recommendation generation run complete"
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(GenerateAccepted {
            status: "generated".to_string(),
            generated_count,
            inserted_count,
            status_url: "/v1/recommendations".to_string(),
        }),
    ))
}

// ---- GET / (cursor pagination) ------------------------------------------

const DEFAULT_PAGE_LIMIT: i64 = 50;
const MAX_PAGE_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse {
    pub items: Vec<JsonValue>,
    /// Opaque cursor for the next page; `null` when the last page was returned.
    pub next_cursor: Option<String>,
}

async fn list_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<ListResponse>, ApiError> {
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    let limit = params
        .limit
        .unwrap_or(DEFAULT_PAGE_LIMIT)
        .clamp(1, MAX_PAGE_LIMIT);

    let after = match params.cursor.as_deref() {
        None => None,
        Some(c) => Some(decode_cursor(c).ok_or_else(|| {
            err(
                StatusCode::BAD_REQUEST,
                "invalid_cursor",
                "malformed page cursor",
            )
        })?),
    };

    // Fetch one extra row to know whether a further page exists.
    let mut rows = state
        .storage
        .recommendations()
        .list_active_paginated(project_uuid, limit + 1, after)
        .await
        .map_err(internal("list recommendations"))?;

    let next_cursor = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|r| encode_cursor(r.generated_at, r.id))
    } else {
        None
    };

    let items = rows.iter().map(row_to_json).collect();
    Ok(Json(ListResponse { items, next_cursor }))
}

// ---- GET /:id -----------------------------------------------------------

async fn detail_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JsonValue>, ApiError> {
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    let rec_id = Uuid::parse_str(&id)
        .map_err(|_| err(StatusCode::BAD_REQUEST, "invalid_id", "id is not a UUID"))?;

    let row = state
        .storage
        .recommendations()
        .find_by_id(rec_id)
        .await
        .map_err(internal("fetch recommendation"))?
        .filter(|r| r.project_id == project_uuid)
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "not_found",
                "recommendation not found",
            )
        })?;

    Ok(Json(row_to_json(&row)))
}

// ---- PATCH /:id/state ---------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TransitionRequest {
    /// Target lifecycle state (`surfaced`/`acknowledged`/`acted`/`measured`/
    /// `dismissed`).
    pub to: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub evidence_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TransitionResponse {
    pub recommendation: JsonValue,
    pub warnings: Vec<JsonValue>,
}

async fn transition_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<TransitionRequest>,
) -> Result<Json<TransitionResponse>, ApiError> {
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    let rec_id = Uuid::parse_str(&id)
        .map_err(|_| err(StatusCode::BAD_REQUEST, "invalid_id", "id is not a UUID"))?;

    let to = parse_state(&req.to).ok_or_else(|| {
        err(
            StatusCode::BAD_REQUEST,
            "invalid_state",
            "unknown target state",
        )
    })?;

    let row = state
        .storage
        .recommendations()
        .find_by_id(rec_id)
        .await
        .map_err(internal("fetch recommendation"))?
        .filter(|r| r.project_id == project_uuid)
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "not_found",
                "recommendation not found",
            )
        })?;

    let from = parse_state(&row.state).ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "corrupt_state",
            "stored state is not a known lifecycle state",
        )
    })?;

    // `mark_acted` is the only transition that carries a warning (evidence
    // missing); every other edge goes through the plain state machine.
    let mut warnings = Vec::new();
    let new_state = if to == LifecycleState::Acted {
        let result = lifecycle::mark_acted(from, req.note.as_deref(), req.evidence_url.as_deref())
            .map_err(transition_conflict)?;
        for w in result.warnings {
            warnings.push(json!({ "kind": w.kind }));
        }
        result.state
    } else {
        lifecycle::transition(from, to).map_err(transition_conflict)?
    };

    let updated = state
        .storage
        .recommendations()
        .update_state(rec_id, project_uuid, new_state.as_str())
        .await
        .map_err(internal("update state"))?
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "not_found",
                "recommendation not found",
            )
        })?;

    // Emit the matching webhook lifecycle event for the externally-observable
    // states. Acknowledged/Dismissed/Stale are internal and not webhook kinds.
    if let Some(event_state) = webhook_event_state(new_state) {
        emit_recommendation_event(
            &state,
            project_uuid,
            updated.id,
            &updated.kind,
            &updated.summary,
            event_state,
        )
        .await;
    }

    Ok(Json(TransitionResponse {
        recommendation: row_to_json(&updated),
        warnings,
    }))
}

// ---- helpers ------------------------------------------------------------

fn internal(context: &'static str) -> impl Fn(anseo_storage::Error) -> ApiError {
    move |e| {
        tracing::error!(error = %e, context, "recommendations route DB error");
        err(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", context)
    }
}

fn transition_conflict(e: lifecycle::LifecycleError) -> ApiError {
    err(StatusCode::CONFLICT, "illegal_transition", &e.to_string())
}

fn parse_state(s: &str) -> Option<LifecycleState> {
    match s {
        "generated" => Some(LifecycleState::Generated),
        "surfaced" => Some(LifecycleState::Surfaced),
        "acknowledged" => Some(LifecycleState::Acknowledged),
        "acted" => Some(LifecycleState::Acted),
        "measured" => Some(LifecycleState::Measured),
        "dismissed" => Some(LifecycleState::Dismissed),
        "stale" => Some(LifecycleState::Stale),
        _ => None,
    }
}

/// The webhook-eligible state string for a transition target, or `None` when
/// the target state is internal (no `recommendation.*` event is emitted).
fn webhook_event_state(state: LifecycleState) -> Option<&'static str> {
    match state {
        LifecycleState::Surfaced => Some("surfaced"),
        LifecycleState::Acted => Some("acted"),
        LifecycleState::Measured => Some("measured"),
        _ => None,
    }
}

fn enum_str<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Project an engine [`Recommendation`] into a `NewRecommendation` row. The
/// engine never sets a non-deterministic plugin source, so `plugin_source` is
/// always `None` here (first-party).
fn rec_to_new_row(rec: &Recommendation, project_id: Uuid) -> NewRecommendation {
    NewRecommendation {
        id: Uuid::from_bytes(rec.id.to_bytes()),
        project_id,
        kind: rec.kind.as_str().to_string(),
        severity: enum_str(&rec.severity),
        confidence_band: enum_str(&rec.confidence_band),
        // Freshly generated rows enter at `generated` (DB lifecycle string).
        state: "generated".to_string(),
        summary: rec.summary.clone(),
        payload: rec.payload.clone(),
        traceability: serde_json::to_value(&rec.traceability).unwrap_or(JsonValue::Null),
        reproducibility_class: enum_str(&rec.reproducibility.class),
        reproducibility_note: rec.reproducibility.note.clone(),
        tags: rec.tags.clone(),
        input_fingerprint: rec.traceability.input_fingerprint.clone(),
        engine_version: rec.engine_version.clone(),
        plugin_source: None,
    }
}

/// REST/wire JSON for a stored recommendation (architecture §8 shape). Mirrors
/// the engine wire envelope plus the DB lifecycle `state`.
fn row_to_json(row: &RecommendationRow) -> JsonValue {
    json!({
        "id": uuid_to_ulid_string(row.id),
        "project_id": uuid_to_ulid_string(row.project_id),
        "kind": row.kind,
        "severity": row.severity,
        "confidence_band": row.confidence_band,
        "state": row.state,
        "summary": row.summary,
        "payload": row.payload,
        "traceability": row.traceability,
        "reproducibility": {
            "class": row.reproducibility_class,
            "note": row.reproducibility_note,
        },
        "tags": row.tags,
        "generated_at": row.generated_at,
        "engine_version": row.engine_version,
    })
}

/// Emit a `recommendation.<state>` lifecycle event: broadcast it for SSE
/// subscribers and fan it out to webhook deliveries (Phase 2 signer + ladder).
async fn emit_recommendation_event(
    state: &AppState,
    project_id: Uuid,
    recommendation_id: Uuid,
    kind: &str,
    summary: &str,
    lifecycle_state: &str,
) {
    let payload = RecommendationPayload {
        event_id: Uuid::new_v4(),
        project_id,
        recommendation_id,
        recommendation_kind: kind.to_string(),
        state: lifecycle_state.to_string(),
        summary: summary.to_string(),
        emitted_at: Utc::now(),
    };
    let event = match lifecycle_state {
        "generated" => LifecycleEvent::RecommendationGenerated(payload),
        "surfaced" => LifecycleEvent::RecommendationSurfaced(payload),
        "acted" => LifecycleEvent::RecommendationActed(payload),
        "measured" => LifecycleEvent::RecommendationMeasured(payload),
        _ => return,
    };

    // SSE (best-effort: no subscribers is fine).
    let _ = state.events.send(event.clone());

    // Webhook fanout — persists `pending` delivery rows the dispatcher signs.
    if let Err(e) =
        anseo_scheduler::webhooks::fanout::enqueue_lifecycle_event(&state.storage, &event).await
    {
        tracing::error!(error = %e, "recommendation webhook fanout failed");
    }
}

fn uuid_to_ulid_string(u: Uuid) -> String {
    ulid::Ulid::from_bytes(u.into_bytes()).to_string()
}

/// Cursor encodes `(generated_at_micros, id)` as `<micros>:<uuid>` — opaque to
/// clients, stable across pages.
fn encode_cursor(generated_at: DateTime<Utc>, id: Uuid) -> String {
    format!("{}:{}", generated_at.timestamp_micros(), id)
}

fn decode_cursor(c: &str) -> Option<(DateTime<Utc>, Uuid)> {
    let (micros, id) = c.split_once(':')?;
    let micros: i64 = micros.parse().ok()?;
    let ts = DateTime::<Utc>::from_timestamp_micros(micros)?;
    let id = Uuid::parse_str(id).ok()?;
    Some((ts, id))
}
