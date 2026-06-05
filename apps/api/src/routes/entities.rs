//! Entity registry API — Story 43.1 / 43.3.
//!
//! `GET /v1/entities/:domain` — internal lookup by normalized domain.
//! Returns the full entity record (display_name, role, claim_status, etc.)
//! or 404 if the domain is not registered.
//!
//! `POST /v1/entities` — register a new entity (upsert-on-conflict).
//! Returns 200 + existing record when the domain is already registered,
//! with `inserted: false` in the body (Story 43.3 AC-1 collision detection).
//!
//! `GET /v1/benchmark/leaderboard` — public leaderboard surface (Story 43.4).
//! Returns domains ranked by citation frequency, enriched with entity
//! registry state (display_name + claim_status).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use opengeo_storage::repositories::entities::{
    display_name_similarity, EntityRecord, AUTO_MERGE_THRESHOLD, REVIEW_QUEUE_THRESHOLD,
};
use serde::{Deserialize, Serialize};
use sqlx::Row as _;

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/entities/:domain", get(get_entity))
        .route("/entities", post(create_entity))
        .route("/benchmark/leaderboard", get(get_leaderboard))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/entities/:domain
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EntityView {
    pub domain: String,
    pub display_name: String,
    pub role: String,
    pub claim_status: String,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub verification_method: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<EntityRecord> for EntityView {
    fn from(r: EntityRecord) -> Self {
        Self {
            domain: r.domain,
            display_name: r.display_name,
            role: r.role,
            claim_status: r.claim_status,
            verified_at: r.verified_at,
            verification_method: r.verification_method,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

async fn get_entity(
    Path(raw_domain): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<EntityView>, (StatusCode, Json<serde_json::Value>)> {
    let domain = opengeo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);
    match state.storage.entities().get(&domain).await {
        Ok(Some(record)) => Ok(Json(EntityView::from(record))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "entity_not_found",
                "domain": domain,
            })),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
        )),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/entities
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub domain: String,
    pub display_name: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "source".to_string()
}

#[derive(Debug, Serialize)]
pub struct CreateEntityResponse {
    pub entity: EntityView,
    /// `true` when the entity was freshly created; `false` when the domain was
    /// already registered (collision — caller should inspect `merge_suggestion`).
    pub inserted: bool,
    /// Present when `inserted = false` and the existing display_name differs
    /// from the requested one. Callers should surface this to operators.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_suggestion: Option<MergeSuggestion>,
}

#[derive(Debug, Serialize)]
pub struct MergeSuggestion {
    pub existing_name: String,
    pub requested_name: String,
    pub similarity_score: u8,
    /// If the score is in the review-queue band, an entry has been created.
    pub queued_for_review: bool,
}

async fn create_entity(
    State(state): State<AppState>,
    Json(body): Json<CreateEntityRequest>,
) -> Result<(StatusCode, Json<CreateEntityResponse>), (StatusCode, Json<serde_json::Value>)> {
    let domain =
        opengeo_storage::repositories::entities::EntityRepo::normalize_domain(&body.domain);
    let display_name = body.display_name.trim().to_string();
    let role = body.role.trim().to_string();

    if display_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "empty_display_name" })),
        ));
    }

    let repo = state.storage.entities();

    let (record, was_inserted) = repo
        .upsert(&domain, &display_name, &role)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
            )
        })?;

    let merge_suggestion = if !was_inserted && record.display_name != display_name {
        let score = display_name_similarity(&record.display_name, &display_name);
        let queued_for_review = score >= REVIEW_QUEUE_THRESHOLD && score < AUTO_MERGE_THRESHOLD;

        if queued_for_review {
            // Enqueue for human review (Story 43.3 AC-2).
            let _ = repo
                .enqueue_dedup_review(
                    &domain,
                    &domain,
                    &display_name,
                    score as i16,
                    "cross_operator_name_mismatch",
                )
                .await;
        }

        Some(MergeSuggestion {
            existing_name: record.display_name.clone(),
            requested_name: display_name,
            similarity_score: score,
            queued_for_review,
        })
    } else {
        None
    };

    let status = if was_inserted {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((
        status,
        Json(CreateEntityResponse {
            entity: EntityView::from(record),
            inserted: was_inserted,
            merge_suggestion,
        }),
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/benchmark/leaderboard (Story 43.4)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct LeaderboardResponse {
    pub items: Vec<LeaderboardEntry>,
    /// Mandatory header copy for the UI (Story 43.4 AC-3).
    pub statement: &'static str,
}

#[derive(Debug, Serialize)]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub domain: String,
    pub display_name: String,
    pub claim_status: String,
    pub role: Option<String>,
    pub citation_count: i64,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn get_leaderboard(
    State(state): State<AppState>,
) -> Result<Json<LeaderboardResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Aggregate citation counts from the citations table (cross-project).
    // For each domain, look up entity registry enrichment.
    let rows = sqlx::query(
        r#"
        SELECT
            domain,
            SUM(frequency)::bigint AS citation_count
        FROM citations
        GROUP BY domain
        ORDER BY citation_count DESC
        LIMIT 100
        "#,
    )
    .fetch_all(state.storage.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
        )
    })?;

    let repo = state.storage.entities();
    let mut items = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let raw_domain: String = row.get("domain");
        let citation_count: i64 = row.get("citation_count");
        let normalized = opengeo_storage::repositories::entities::EntityRepo::normalize_domain(&raw_domain);
        let (display_name, claim_status) = repo.resolve_display(&normalized).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "storage_error", "message": e.to_string() })),
            )
        })?;

        // Fetch role + verified_at from entity registry if present.
        let (role, verified_at) = match repo.get(&normalized).await {
            Ok(Some(e)) => (Some(e.role), e.verified_at),
            _ => (None, None),
        };

        items.push(LeaderboardEntry {
            rank: (i as i64) + 1,
            domain: normalized,
            display_name,
            claim_status,
            role,
            citation_count,
            verified_at,
        });
    }

    Ok(Json(LeaderboardResponse {
        items,
        statement: "Rankings are based on measured data. Entities cannot influence their own rank.",
    }))
}
