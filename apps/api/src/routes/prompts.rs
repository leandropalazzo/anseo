//! `GET /v1/prompts/run-summary` — Story 0.9 substrate.
//!
//! Aggregates run state per declared prompt within a configurable
//! window (`since`, RFC3339; default = 30 days ago). The shape is
//! consumed by:
//!
//! - MCP tool wrappers needing "is this prompt healthy / how often does
//!   it run / which providers" without listing individual runs.
//! - The Extension's Prompt picker which surfaces avg latency + success
//!   rate before the operator triggers a new run.
//!
//! Rows where the prompt has had zero runs in the window are included
//! with `run_count = 0` and `last_run_at = null` so the Extension can
//! still render the row. Determinism contract: items are ordered by
//! prompt name ascending.
//!
//! `X-Anseo-Project` is accepted but not consumed at this layer.

use anseo_core::{prompt_id_for, ProjectId, PromptId, ProviderName};
use anseo_providers::ProviderRequest;
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::middleware::auth::AuthenticatedProject;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/prompts/run-summary", get(run_summary))
        .route("/prompts/kpi-trend", get(kpi_trend_handler))
        .route("/prompts/tag-summary", get(tag_summary))
        .route("/prompts", get(list_prompts).post(create_prompt))
        .route("/prompts/suggest", post(suggest_prompts))
        .route(
            "/prompts/:id",
            axum::routing::put(update_prompt).delete(delete_prompt),
        )
}

type ApiError = (StatusCode, Json<serde_json::Value>);

fn err(code: StatusCode, error: &str, message: String) -> ApiError {
    (
        code,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

fn storage_err(e: impl std::fmt::Display) -> ApiError {
    err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "storage_error",
        e.to_string(),
    )
}

#[derive(Debug, Serialize)]
pub struct PromptView {
    pub id: String,
    pub name: String,
    pub text: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct PromptCreate {
    pub name: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PromptUpdate {
    pub name: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PromptMutationResult {
    pub id: String,
    pub name: String,
    pub text: String,
    pub tags: Vec<String>,
    /// `true` when the name changed and thus the prompt id was re-derived.
    pub renamed: bool,
}

/// Trim, drop empties, and dedupe tags case-insensitively (first spelling wins).
fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for t in tags {
        let trimmed = t.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_lowercase()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

fn parse_prompt_id(raw: &str) -> Result<PromptId, ApiError> {
    raw.parse::<PromptId>().map_err(|_| {
        err(
            StatusCode::BAD_REQUEST,
            "invalid_id",
            format!("`{raw}` is not a valid prompt id"),
        )
    })
}

/// Resolve the brand name for `project_id` from the DB (authoritative), falling
/// back to the boot-configured project name when no row exists yet. Prompt ids
/// fold in the brand name, so this must track the *resolved* project — not a
/// single boot-pinned value — for multi-project correctness.
async fn brand_name_for(state: &AppState, project_id: ProjectId) -> String {
    match state.storage.projects().get_brand(project_id).await {
        Ok(Some(row)) => row.name,
        _ => state.configured_project.as_str().to_string(),
    }
}

async fn list_prompts(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
) -> Result<Json<Vec<PromptView>>, ApiError> {
    let rows = state
        .storage
        .prompts()
        .list_by_project(project_id)
        .await
        .map_err(storage_err)?;
    Ok(Json(
        rows.into_iter()
            .map(|p| PromptView {
                id: p.id.to_string(),
                name: p.name,
                text: p.text,
                tags: p.tags,
                created_at: p.created_at,
            })
            .collect(),
    ))
}

async fn create_prompt(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Json(body): Json<PromptCreate>,
) -> Result<(StatusCode, Json<PromptMutationResult>), ApiError> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "empty_name",
            "prompt name must not be empty".to_string(),
        ));
    }
    let brand = brand_name_for(&state, project_id).await;
    let id = prompt_id_for(&brand, &name);
    let tags = normalize_tags(&body.tags);

    let prompts = state.storage.prompts();
    if prompts.get(id).await.map_err(storage_err)?.is_some()
        || prompts
            .find_by_name(project_id, &name)
            .await
            .map_err(storage_err)?
            .is_some()
    {
        return Err(err(
            StatusCode::CONFLICT,
            "prompt_exists",
            format!("a prompt named `{name}` already exists"),
        ));
    }

    prompts
        .insert(&anseo_storage::models::PromptRow {
            id,
            project_id,
            name: name.clone(),
            text: body.text.clone(),
            tags: tags.clone(),
            organization_id: None,
            tenant_id: None,
            created_at: Utc::now(),
        })
        .await
        .map_err(storage_err)?;

    Ok((
        StatusCode::CREATED,
        Json(PromptMutationResult {
            id: id.to_string(),
            name,
            text: body.text,
            tags,
            renamed: false,
        }),
    ))
}

async fn update_prompt(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PromptUpdate>,
) -> Result<Json<PromptMutationResult>, ApiError> {
    let current_id = parse_prompt_id(&id)?;
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "empty_name",
            "prompt name must not be empty".to_string(),
        ));
    }
    let prompts = state.storage.prompts();
    let existing = prompts
        .get(current_id)
        .await
        .map_err(storage_err)?
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                "prompt_not_found",
                format!("no prompt with id `{id}`"),
            )
        })?;
    if existing.project_id != project_id {
        return Err(err(
            StatusCode::NOT_FOUND,
            "prompt_not_found",
            format!("no prompt with id `{id}`"),
        ));
    }

    let brand = brand_name_for(&state, project_id).await;
    let new_id = prompt_id_for(&brand, &name);
    let renamed = new_id != current_id;
    let tags = normalize_tags(&body.tags);

    if renamed {
        // Renaming re-derives the prompt id (id folds in the prompt name).
        // Mirror the brand rename rule: only safe when the prompt has no runs.
        let runs = prompts
            .prompt_run_count(current_id)
            .await
            .map_err(storage_err)?;
        if runs > 0 {
            return Err(err(
                StatusCode::CONFLICT,
                "rename_blocked_has_runs",
                format!(
                    "renaming this prompt changes its id; it has {runs} run(s). Re-keying \
                     existing runs is not supported — rename is only allowed before the first run."
                ),
            ));
        }
        if prompts.get(new_id).await.map_err(storage_err)?.is_some() {
            return Err(err(
                StatusCode::CONFLICT,
                "prompt_exists",
                format!("a prompt named `{name}` already exists"),
            ));
        }
        prompts
            .rename_on_empty(current_id, new_id, &name, &body.text, &tags)
            .await
            .map_err(storage_err)?;
    } else {
        prompts
            .update_content(current_id, &body.text, &tags)
            .await
            .map_err(storage_err)?;
    }

    Ok(Json(PromptMutationResult {
        id: new_id.to_string(),
        name,
        text: body.text,
        tags,
        renamed,
    }))
}

async fn delete_prompt(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let prompt_id = parse_prompt_id(&id)?;
    let prompts = state.storage.prompts();
    let existing = prompts.get(prompt_id).await.map_err(storage_err)?;
    match existing {
        Some(p) if p.project_id == project_id => {
            let runs = prompts
                .prompt_run_count(prompt_id)
                .await
                .map_err(storage_err)?;
            if runs > 0 {
                return Err(err(
                    StatusCode::CONFLICT,
                    "delete_blocked_has_runs",
                    format!("this prompt has {runs} run(s) and cannot be deleted; its run history references it."),
                ));
            }
            prompts.delete(prompt_id).await.map_err(storage_err)?;
            Ok(StatusCode::NO_CONTENT)
        }
        _ => Err(err(
            StatusCode::NOT_FOUND,
            "prompt_not_found",
            format!("no prompt with id `{id}`"),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct RunSummaryQuery {
    /// RFC3339 lower bound. Defaults to now()-30d.
    pub since: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptRunSummaryItem {
    pub prompt: String,
    pub run_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<DateTime<Utc>>,
    /// 0.0..=1.0; `None` if `run_count == 0`.
    pub success_rate: Option<f64>,
    /// Mean (finished_at - started_at) in ms over `ok` runs; `None` if
    /// no completed runs.
    pub avg_latency_ms: Option<f64>,
    /// Distinct providers observed in the window, sorted ascending.
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummaryResponse {
    pub items: Vec<PromptRunSummaryItem>,
    /// Echo of the effective lower bound the response was computed
    /// against. Lets clients render "since YYYY-MM-DD" without
    /// re-deriving the default.
    pub since: DateTime<Utc>,
}

async fn run_summary(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<RunSummaryQuery>,
) -> Result<Json<RunSummaryResponse>, (StatusCode, Json<serde_json::Value>)> {
    let since = match q.since.as_deref() {
        None => Utc::now() - Duration::days(30),
        Some(raw) => match DateTime::parse_from_rfc3339(raw) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_since",
                        "message": format!("`since` must be RFC3339: {e}"),
                    })),
                ));
            }
        },
    };

    let items = fetch_summary(&state.storage, project_id, since)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "prompt run-summary fetch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": "prompt run-summary fetch failed",
                })),
            )
        })?;

    Ok(Json(RunSummaryResponse { items, since }))
}

#[derive(Debug, Deserialize)]
pub struct KpiTrendQuery {
    /// Window in hours (default 168 = 7d), clamped server-side to [1, 8760].
    pub hours: Option<i32>,
}

/// `GET /v1/prompts/kpi-trend` — hourly project-wide KPI series (run_count,
/// success_rate, avg_latency_ms) for the Overview tile sparklines.
async fn kpi_trend_handler(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<KpiTrendQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let points = anseo_analytics::kpi_trend(&state.storage, project_id, q.hours.unwrap_or(168))
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "kpi trend fetch failed");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "kpi trend fetch failed".to_string(),
            )
        })?;
    Ok(Json(serde_json::json!({ "points": points })))
}

async fn fetch_summary(
    storage: &anseo_storage::Storage,
    project_id: ProjectId,
    since: DateTime<Utc>,
) -> Result<Vec<PromptRunSummaryItem>, sqlx::Error> {
    // LEFT JOIN so prompts with zero runs in window still appear.
    // `array_agg(DISTINCT ...)` gives the per-prompt provider set.
    let rows = sqlx::query(
        r#"
        SELECT
            p.name                                                       AS prompt,
            COUNT(pr.id)::bigint                                         AS run_count,
            MAX(pr.started_at)                                           AS last_run_at,
            SUM(CASE WHEN pr.status = 'ok' THEN 1 ELSE 0 END)::bigint    AS ok_count,
            AVG(
              CASE
                WHEN pr.status = 'ok' AND pr.finished_at IS NOT NULL
                THEN EXTRACT(EPOCH FROM (pr.finished_at - pr.started_at)) * 1000.0
                ELSE NULL
              END
            )::double precision                                          AS avg_latency_ms,
            ARRAY(
              SELECT DISTINCT pr2.provider
              FROM prompt_runs pr2
              WHERE pr2.prompt_id = p.id
                AND pr2.started_at >= $2
              ORDER BY pr2.provider
            )                                                            AS providers
        FROM prompts p
        LEFT JOIN prompt_runs pr
          ON pr.prompt_id = p.id
         AND pr.started_at >= $2
        WHERE p.project_id = $1
        GROUP BY p.id, p.name
        ORDER BY p.name ASC
        "#,
    )
    .bind(project_id)
    .bind(since)
    .fetch_all(storage.pool())
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        let prompt: String = r.try_get("prompt")?;
        let run_count: i64 = r.try_get("run_count")?;
        let last_run_at: Option<DateTime<Utc>> = r.try_get("last_run_at")?;
        let ok_count: i64 = r.try_get("ok_count")?;
        let avg_latency_ms: Option<f64> = r.try_get("avg_latency_ms")?;
        let providers: Vec<String> = r.try_get("providers")?;

        let success_rate = if run_count > 0 {
            Some(ok_count as f64 / run_count as f64)
        } else {
            None
        };

        items.push(PromptRunSummaryItem {
            prompt,
            run_count,
            last_run_at,
            success_rate,
            avg_latency_ms,
            providers,
        });
    }
    Ok(items)
}

#[derive(Debug, Clone, Serialize)]
pub struct TagSummaryItem {
    pub tag: String,
    /// Prompts in the project carrying this tag.
    pub prompt_count: i64,
    /// Runs (in window) across those prompts.
    pub run_count: i64,
    /// 0.0..=1.0; `None` if `run_count == 0`.
    pub success_rate: Option<f64>,
    /// Distinct providers observed across those prompts' runs, sorted asc.
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagSummaryResponse {
    pub items: Vec<TagSummaryItem>,
    pub since: DateTime<Utc>,
}

/// `GET /v1/prompts/tag-summary?since=<RFC3339>` — per-tag rollup for the
/// Overview. A prompt may carry several tags, so prompts are unnested by tag;
/// run metrics are aggregated over the prompts under each tag.
async fn tag_summary(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Query(q): Query<RunSummaryQuery>,
) -> Result<Json<TagSummaryResponse>, ApiError> {
    let since = match q.since.as_deref() {
        None => Utc::now() - Duration::days(30),
        Some(raw) => match DateTime::parse_from_rfc3339(raw) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                return Err(err(
                    StatusCode::BAD_REQUEST,
                    "invalid_since",
                    format!("`since` must be RFC3339: {e}"),
                ));
            }
        },
    };

    let pid = uuid::Uuid::from_bytes(project_id.into_ulid().to_bytes());
    let rows = sqlx::query(
        r#"
        SELECT
            tag,
            COUNT(DISTINCT p.id)::bigint                                 AS prompt_count,
            COUNT(pr.id)::bigint                                         AS run_count,
            SUM(CASE WHEN pr.status = 'ok' THEN 1 ELSE 0 END)::bigint    AS ok_count,
            ARRAY(
              SELECT DISTINCT pr2.provider
              FROM prompts p2
              JOIN prompt_runs pr2 ON pr2.prompt_id = p2.id
              WHERE p2.project_id = $1
                AND tag = ANY(p2.tags)
                AND pr2.started_at >= $2
              ORDER BY pr2.provider
            )                                                            AS providers
        FROM prompts p
        CROSS JOIN LATERAL unnest(p.tags) AS tag
        LEFT JOIN prompt_runs pr
          ON pr.prompt_id = p.id
         AND pr.started_at >= $2
        WHERE p.project_id = $1
        GROUP BY tag
        ORDER BY tag ASC
        "#,
    )
    .bind(pid)
    .bind(since)
    .fetch_all(state.storage.pool())
    .await
    .map_err(storage_err)?;

    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        let tag: String = r.try_get("tag").map_err(storage_err)?;
        let prompt_count: i64 = r.try_get("prompt_count").map_err(storage_err)?;
        let run_count: i64 = r.try_get("run_count").map_err(storage_err)?;
        let ok_count: i64 = r.try_get("ok_count").map_err(storage_err)?;
        let providers: Vec<String> = r.try_get("providers").map_err(storage_err)?;
        let success_rate = if run_count > 0 {
            Some(ok_count as f64 / run_count as f64)
        } else {
            None
        };
        items.push(TagSummaryItem {
            tag,
            prompt_count,
            run_count,
            success_rate,
            providers,
        });
    }

    Ok(Json(TagSummaryResponse { items, since }))
}

#[derive(Debug, Deserialize)]
pub struct SuggestPromptsRequest {
    /// Wire name of the provider to ask (must have a configured key).
    pub provider: String,
}

#[derive(Debug, Serialize)]
pub struct SuggestedPrompt {
    pub name: String,
    pub text: String,
    /// Resolved tags: the matched existing project tag when the model's
    /// proposed category matches one, otherwise the literal "AUTO".
    pub tags: Vec<String>,
}

/// Parser intermediate before tags are resolved against the project's
/// existing tag vocabulary.
struct ParsedPrompt {
    name: String,
    text: String,
    category: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SuggestPromptsResult {
    pub prompts: Vec<SuggestedPrompt>,
    pub provider: String,
    pub model: String,
}

/// `POST /v1/prompts/suggest` — ask a configured LLM for a set of tracking
/// prompts grounded in the operator's brand + competitors. The operator picks
/// the provider in the UI; we look it up in the boot-loaded registry, send a
/// single completion, and parse a JSON array of `{name, text}` objects.
/// Nothing is persisted — suggestions are returned for review; the editor
/// creates the ones the operator keeps.
async fn suggest_prompts(
    Extension(AuthenticatedProject(project_id)): Extension<AuthenticatedProject>,
    State(state): State<AppState>,
    Json(body): Json<SuggestPromptsRequest>,
) -> Result<Json<SuggestPromptsResult>, ApiError> {
    let Some(provider_name) = ProviderName::parse(&body.provider) else {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "invalid_provider",
            format!("unknown provider `{}`", body.provider),
        ));
    };
    let Some(registry) = state.provider_registry.as_ref() else {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "no_registry",
            "API booted without a provider registry; configure a provider key first".to_string(),
        ));
    };
    let Some(provider) = registry.get(&provider_name) else {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "provider_not_configured",
            format!("provider `{}` has no configured API key", body.provider),
        ));
    };

    // Ground the suggestions in the operator's actual brand + variants +
    // competitors (DB is authoritative), so the prompts track real rivals.
    let brand_name = brand_name_for(&state, project_id).await;
    let (variants, competitors) = match state.storage.projects().get_brand(project_id).await {
        Ok(Some(row)) => {
            let comps: Vec<String> =
                serde_json::from_value::<Vec<anseo_core::CompetitorConfig>>(row.competitors)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|c| c.name)
                    .collect();
            (row.variants, comps)
        }
        _ => (Vec::new(), Vec::new()),
    };

    // Existing tag vocabulary across the project's prompts — the model is asked
    // to reuse one of these when a generated prompt fits, so AUTO tags can find
    // a best match instead of always creating a fresh label.
    let existing_tags: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        let mut tags = Vec::new();
        if let Ok(rows) = state.storage.prompts().list_by_project(project_id).await {
            for row in rows {
                for t in row.tags {
                    if seen.insert(t.to_lowercase()) {
                        tags.push(t);
                    }
                }
            }
        }
        tags
    };

    let model = provider_name.default_model().to_string();
    let alias_hint = if variants.is_empty() {
        String::new()
    } else {
        format!(" (also known as: {})", variants.join(", "))
    };
    let competitor_hint = if competitors.is_empty() {
        String::new()
    } else {
        format!(" Known competitors include: {}.", competitors.join(", "))
    };
    let category_hint = if existing_tags.is_empty() {
        " Set \"category\" to a short topical label for each prompt.".to_string()
    } else {
        format!(
            " For \"category\", reuse one of these existing tags when it fits: {}. \
             Only invent a new short label if none fit.",
            existing_tags.join(", ")
        )
    };
    let prompt = format!(
        "You are helping track how the brand \"{brand_name}\"{alias_hint} appears in AI \
         search answers.{competitor_hint} Generate a set of natural-language search prompts \
         that a real user might ask an AI assistant, where the answer would likely mention \
         \"{brand_name}\" or its competitors. Cover category comparisons, \"best X for Y\" \
         questions, alternatives, and use-case queries. Respond with ONLY a JSON array of 5 to 8 \
         objects, each {{\"name\": \"<short kebab-case slug>\", \"text\": \"<the prompt>\", \
         \"category\": \"<topical label>\"}}.{category_hint} \
         No prose, no markdown, no code fences."
    );

    let request = ProviderRequest::new(prompt, model.clone())
        .with_timeout(std::time::Duration::from_secs(30));
    let response = provider.run(request).await.map_err(|e| {
        err(
            StatusCode::BAD_GATEWAY,
            "provider_error",
            format!("provider call failed: {e}"),
        )
    })?;

    let parsed = parse_suggested_prompts(&response.message_text)
        .map_err(|m| err(StatusCode::BAD_GATEWAY, "unparseable_suggestion", m))?;

    let prompts = parsed
        .into_iter()
        .map(|p| SuggestedPrompt {
            name: p.name,
            text: p.text,
            tags: resolve_tags(p.category.as_deref(), &existing_tags),
        })
        .collect();

    Ok(Json(SuggestPromptsResult {
        prompts,
        provider: body.provider,
        model,
    }))
}

/// Extract a JSON array of `{name, text, category}` prompt objects from a model
/// response, tolerating stray prose or ```json fences around the array.
fn parse_suggested_prompts(text: &str) -> Result<Vec<ParsedPrompt>, String> {
    let start = text.find('[');
    let end = text.rfind(']');
    let slice = match (start, end) {
        (Some(s), Some(e)) if e > s => &text[s..=e],
        _ => return Err("model response did not contain a JSON array".to_string()),
    };
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(slice).map_err(|e| format!("could not parse JSON array: {e}"))?;
    let prompts: Vec<ParsedPrompt> = raw
        .into_iter()
        .filter_map(|v| {
            let obj = v.as_object()?;
            let text = obj.get("text").and_then(|t| t.as_str())?.trim().to_string();
            if text.is_empty() {
                return None;
            }
            // Name is optional; derive a slug from the text when absent.
            let name = obj
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| slug_from_text(&text));
            let category = obj
                .get("category")
                .and_then(|c| c.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Some(ParsedPrompt {
                name,
                text,
                category,
            })
        })
        .collect();
    if prompts.is_empty() {
        return Err("model returned an empty prompt list".to_string());
    }
    Ok(prompts)
}

/// Resolve a model-proposed category into a tag set. When the category matches
/// an existing project tag (case-insensitive), reuse the existing spelling so
/// rollups stay consistent; otherwise fall back to the literal "AUTO".
fn resolve_tags(category: Option<&str>, existing: &[String]) -> Vec<String> {
    match category {
        Some(cat) => {
            let cat_lc = cat.to_lowercase();
            match existing.iter().find(|t| t.to_lowercase() == cat_lc) {
                Some(matched) => vec![matched.clone()],
                None => vec!["AUTO".to_string()],
            }
        }
        None => vec!["AUTO".to_string()],
    }
}

/// Build a short kebab-case slug from prompt text (first few words).
fn slug_from_text(text: &str) -> String {
    let slug: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "prompt".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_serializes_with_required_fields() {
        let item = PromptRunSummaryItem {
            prompt: "vector-db".into(),
            run_count: 14,
            last_run_at: Some(Utc::now()),
            success_rate: Some(0.93),
            avg_latency_ms: Some(1240.0),
            providers: vec!["anthropic".into(), "openai".into()],
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["prompt"], "vector-db");
        assert_eq!(v["run_count"], 14);
        assert_eq!(v["success_rate"], serde_json::json!(0.93));
        assert_eq!(v["providers"][1], "openai");
    }

    #[test]
    fn item_with_no_runs_serializes_nulls() {
        let item = PromptRunSummaryItem {
            prompt: "dormant".into(),
            run_count: 0,
            last_run_at: None,
            success_rate: None,
            avg_latency_ms: None,
            providers: vec![],
        };
        let v = serde_json::to_value(&item).unwrap();
        assert_eq!(v["run_count"], 0);
        assert!(v["success_rate"].is_null());
        assert!(v["avg_latency_ms"].is_null());
        // last_run_at uses skip_serializing_if so it may be omitted.
        assert!(v.get("last_run_at").is_none_or(|x| x.is_null()));
    }

    #[test]
    fn since_query_parses_rfc3339() {
        // Sanity check that DateTime::parse_from_rfc3339 accepts the
        // documented shape — guards against accidental tightening.
        let parsed = DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z").unwrap();
        assert_eq!(
            parsed
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "2026-05-29T12:00:00Z"
        );
    }
}
