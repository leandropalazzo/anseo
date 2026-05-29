//! Phase 2 Story 10.4 — REST surface for declared schedules.
//!
//! Endpoints (all `X-OpenGEO-API-Key`-gated):
//!
//! - `GET    /v1/schedules`         — list schedules for the authenticated project
//! - `GET    /v1/schedules/:id`     — fetch one schedule
//! - `POST   /v1/schedules`         — declare a new schedule (validates density
//!                                    + cost caps, mirrors the CLI's checks)
//! - `PUT    /v1/schedules/:id`     — update a schedule (today: pause/unpause +
//!                                    cost-ack timestamp)
//! - `DELETE /v1/schedules/:id`     — remove a schedule (cascades schedule_ticks)
//!
//! The write surface validates against the same `project_schedule_cost`
//! helper the CLI uses (`crates/scheduler/src/lib.rs`), so YAML-side and
//! API-side rejections agree.

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use opengeo_core::{ProjectId, ProviderName, ScheduleConfig};
use opengeo_providers::cost::DEFAULT_PROJECT_MONTHLY_CAP_USD;
use opengeo_scheduler::{project_schedule_cost, ScheduleValidationError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/schedules", get(list_schedules).post(create_schedule))
        .route(
            "/schedules/:id",
            get(get_schedule).put(update_schedule).delete(delete_schedule),
        )
}

#[derive(Debug, Serialize)]
pub struct ScheduleSummary {
    pub id: Uuid,
    pub name: String,
    pub cron: String,
    pub prompts: Vec<String>,
    pub providers: Vec<String>,
    pub debounce_minutes: i32,
    pub projected_monthly_usd: Option<f64>,
    pub projection_acknowledged_at: Option<DateTime<Utc>>,
    pub paused: bool,
    pub created_at: DateTime<Utc>,
    pub last_tick_at: Option<DateTime<Utc>>,
    pub last_tick_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListSchedulesResponse {
    pub schedules: Vec<ScheduleSummary>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub cron: String,
    pub prompts: Vec<String>,
    /// Provider wire names (e.g. `"openai"`, `"anthropic"`); validated
    /// against `ProviderName::parse`.
    pub providers: Vec<String>,
    #[serde(default = "default_debounce")]
    pub debounce_minutes: u32,
    /// Set to `true` to ack a projected monthly cost above the project
    /// cap. Mirrors the CLI's `--allow-expensive`.
    #[serde(default)]
    pub allow_expensive: bool,
}

fn default_debounce() -> u32 {
    opengeo_core::DEFAULT_SCHEDULE_DEBOUNCE_MINUTES
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    /// Toggle the paused flag. `None` = leave unchanged.
    #[serde(default)]
    pub paused: Option<bool>,
    /// Re-ack a projected monthly cost (e.g. after a cap change).
    /// Sets `projection_acknowledged_at` to `now()` when `true`.
    #[serde(default)]
    pub acknowledge_projection: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ProjectionPreview {
    pub ticks_per_day: f64,
    pub projected_monthly_usd: f64,
    pub above_cap: bool,
    pub cap_usd: f64,
}

fn err(status: StatusCode, error: &str, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

async fn list_schedules(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
) -> Result<Json<ListSchedulesResponse>, (StatusCode, Json<serde_json::Value>)> {
    let schedules = fetch_schedules(&state, project_id, None).await?;
    Ok(Json(ListSchedulesResponse { schedules }))
}

async fn get_schedule(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ScheduleSummary>, (StatusCode, Json<serde_json::Value>)> {
    let mut schedules = fetch_schedules(&state, project_id, Some(id)).await?;
    schedules.pop().map(Json).ok_or_else(|| {
        err(
            StatusCode::NOT_FOUND,
            "schedule_not_found",
            "no schedule with that id is declared in this project",
        )
    })
}

async fn create_schedule(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Json(request): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ScheduleSummary>), (StatusCode, Json<serde_json::Value>)> {
    let name = request.name.trim().to_string();
    if name.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "invalid_name", "`name` is required"));
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "`name` must be slug-safe (lowercase ASCII + digits + hyphens)",
        ));
    }
    if request.prompts.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "missing_prompts",
            "`prompts` must list at least one declared prompt name",
        ));
    }
    if request.providers.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "missing_providers",
            "`providers` must list at least one provider",
        ));
    }
    let mut parsed_providers: Vec<ProviderName> = Vec::with_capacity(request.providers.len());
    for p in &request.providers {
        let Some(parsed) = ProviderName::parse(p) else {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "invalid_provider",
                &format!(
                    "unsupported provider `{p}`; expected one of {}",
                    ProviderName::all_wire_names().join(", ")
                ),
            ));
        };
        if !parsed_providers.contains(&parsed) {
            parsed_providers.push(parsed);
        }
    }

    let candidate = ScheduleConfig {
        name: name.clone(),
        cron: request.cron.clone(),
        prompts: request.prompts.clone(),
        providers: parsed_providers,
        debounce_minutes: request.debounce_minutes,
        projection_acknowledged_at: None,
    };
    let projection = project_schedule_cost(&candidate).map_err(map_validation_error)?;
    let above_cap = projection.cost.projected_monthly_usd > DEFAULT_PROJECT_MONTHLY_CAP_USD;
    if above_cap && !request.allow_expensive {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "projection_above_cap",
            &format!(
                "projected monthly cost ${:.2} exceeds cap ${:.2}; resubmit with `allow_expensive: true` to acknowledge",
                projection.cost.projected_monthly_usd, DEFAULT_PROJECT_MONTHLY_CAP_USD
            ),
        ));
    }

    let id = Uuid::new_v4();
    let acknowledged_at = if above_cap { Some(Utc::now()) } else { None };
    let prompts_json = serde_json::to_value(&candidate.prompts).expect("Vec<String> serializes");
    let providers_json = serde_json::to_value(
        &candidate
            .providers
            .iter()
            .map(|p| p.as_wire_str())
            .collect::<Vec<_>>(),
    )
    .expect("Vec<&str> serializes");

    let inserted = sqlx::query(
        r#"
        INSERT INTO schedules (
            id, project_id, name, cron, prompts, providers,
            debounce_minutes, projected_monthly_usd,
            projection_acknowledged_at, paused
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, FALSE)
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(&name)
    .bind(&candidate.cron)
    .bind(&prompts_json)
    .bind(&providers_json)
    .bind(candidate.debounce_minutes as i32)
    .bind(projection.cost.projected_monthly_usd)
    .bind(acknowledged_at)
    .execute(state.storage.pool())
    .await;

    if let Err(sqlx::Error::Database(db_err)) = &inserted {
        if db_err.code().as_deref() == Some("23505") {
            return Err(err(
                StatusCode::CONFLICT,
                "duplicate_schedule",
                &format!("schedule `{name}` already exists in this project"),
            ));
        }
    }
    inserted.map_err(|e| {
        tracing::error!(error = %e, "schedule insert failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "schedule insert failed",
        )
    })?;

    let mut schedules = fetch_schedules(&state, project_id, Some(id)).await?;
    let summary = schedules.pop().ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "schedule was inserted but immediately disappeared",
        )
    })?;
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn update_schedule(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateScheduleRequest>,
) -> Result<Json<ScheduleSummary>, (StatusCode, Json<serde_json::Value>)> {
    if request.paused.is_none() && request.acknowledge_projection.is_none() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "no_op",
            "at least one of `paused` or `acknowledge_projection` is required",
        ));
    }

    let now = Utc::now();
    let result = sqlx::query(
        r#"
        UPDATE schedules
        SET
            paused = COALESCE($3, paused),
            projection_acknowledged_at = CASE
                WHEN $4::boolean IS TRUE THEN $5
                ELSE projection_acknowledged_at
            END
        WHERE id = $1 AND project_id = $2
        "#,
    )
    .bind(id)
    .bind(project_id)
    .bind(request.paused)
    .bind(request.acknowledge_projection)
    .bind(now)
    .execute(state.storage.pool())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "schedule update failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "schedule update failed",
        )
    })?;
    if result.rows_affected() == 0 {
        return Err(err(
            StatusCode::NOT_FOUND,
            "schedule_not_found",
            "no schedule with that id is declared in this project",
        ));
    }
    let mut schedules = fetch_schedules(&state, project_id, Some(id)).await?;
    schedules.pop().map(Json).ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "schedule disappeared after update",
        )
    })
}

async fn delete_schedule(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let result = sqlx::query(
        r#"DELETE FROM schedules WHERE id = $1 AND project_id = $2"#,
    )
    .bind(id)
    .bind(project_id)
    .execute(state.storage.pool())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "schedule delete failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "schedule delete failed",
        )
    })?;
    if result.rows_affected() == 0 {
        return Err(err(
            StatusCode::NOT_FOUND,
            "schedule_not_found",
            "no schedule with that id is declared in this project",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

fn map_validation_error(e: ScheduleValidationError) -> (StatusCode, Json<serde_json::Value>) {
    match &e {
        ScheduleValidationError::UnsupportedCadence(expr) => err(
            StatusCode::BAD_REQUEST,
            "unsupported_cadence",
            &format!("`cron` `{expr}` is not a supported cadence"),
        ),
        ScheduleValidationError::PerScheduleHourlyCap { ticks_per_hour, cap, .. } => err(
            StatusCode::BAD_REQUEST,
            "schedule_density_cap",
            &format!(
                "schedule would tick {ticks_per_hour:.2}×/hour, above the cap of {cap:.2}/hour"
            ),
        ),
        ScheduleValidationError::ProviderDailyCap { provider, ticks_per_day, cap, .. } => err(
            StatusCode::BAD_REQUEST,
            "provider_density_cap",
            &format!(
                "provider `{}` would tick {ticks_per_day:.2}×/day, above the cap of {cap:.2}/day",
                provider.as_wire_str()
            ),
        ),
    }
}

async fn fetch_schedules(
    state: &AppState,
    project_id: ProjectId,
    only_id: Option<Uuid>,
) -> Result<Vec<ScheduleSummary>, (StatusCode, Json<serde_json::Value>)> {
    struct Raw {
        id: Uuid,
        name: String,
        cron: String,
        prompts: serde_json::Value,
        providers: serde_json::Value,
        debounce_minutes: i32,
        projected_monthly_usd: Option<f64>,
        projection_acknowledged_at: Option<DateTime<Utc>>,
        paused: bool,
        created_at: DateTime<Utc>,
        last_tick_at: Option<DateTime<Utc>>,
        last_tick_status: Option<String>,
    }
    let raw = sqlx::query_as!(
        Raw,
        r#"
        SELECT
            s.id                              AS "id!: Uuid",
            s.name                            AS "name!: String",
            s.cron                            AS "cron!: String",
            s.prompts                         AS "prompts!: serde_json::Value",
            s.providers                       AS "providers!: serde_json::Value",
            s.debounce_minutes                AS "debounce_minutes!: i32",
            s.projected_monthly_usd           AS "projected_monthly_usd: f64",
            s.projection_acknowledged_at      AS "projection_acknowledged_at: DateTime<Utc>",
            s.paused                          AS "paused!: bool",
            s.created_at                      AS "created_at!: DateTime<Utc>",
            (
                SELECT MAX(t.tick_ts)
                FROM schedule_ticks t
                WHERE t.schedule_id = s.id
            ) AS "last_tick_at: DateTime<Utc>",
            (
                SELECT t.status
                FROM schedule_ticks t
                WHERE t.schedule_id = s.id
                ORDER BY t.tick_ts DESC
                LIMIT 1
            ) AS "last_tick_status: String"
        FROM schedules s
        WHERE s.project_id = $1
          AND ($2::uuid IS NULL OR s.id = $2)
        ORDER BY s.created_at DESC
        LIMIT 500
        "#,
        project_id as ProjectId,
        only_id,
    )
    .fetch_all(state.storage.pool())
    .await
    .map_err(|e| {
        tracing::error!(error = %e, route = "schedules", "fetch failed");
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "schedule fetch failed",
        )
    })?;

    Ok(raw
        .into_iter()
        .map(|r| ScheduleSummary {
            id: r.id,
            name: r.name,
            cron: r.cron,
            prompts: serde_json::from_value(r.prompts).unwrap_or_default(),
            providers: serde_json::from_value(r.providers).unwrap_or_default(),
            debounce_minutes: r.debounce_minutes,
            projected_monthly_usd: r.projected_monthly_usd,
            projection_acknowledged_at: r.projection_acknowledged_at,
            paused: r.paused,
            created_at: r.created_at,
            last_tick_at: r.last_tick_at,
            last_tick_status: r.last_tick_status,
        })
        .collect())
}
