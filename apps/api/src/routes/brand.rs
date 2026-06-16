//! `GET`/`PUT /v1/setup/brand` — DB-authoritative brand config.
//!
//! Phase 1 stored brand config (name, variants, competitors) only in
//! `anseo.yaml`. The dashboard now edits it, so the DB `projects` row is the
//! source of truth: `GET` reads it, `PUT` writes it.
//!
//! Identity coupling: `project_id` (and every `prompt_id`) is a stable hash of
//! `brand.name`. Editing only variants/competitors is an in-place update.
//! Changing the name re-derives `project_id` — which we permit ONLY when the
//! project has zero `prompt_runs` (the fresh-start case), re-keying the project
//! and all prompt ids in one transaction. A rename with existing runs returns
//! 409; the full cascade re-key is a deferred follow-up. After a rename the
//! response carries `restart_required: true` — the running API binds its
//! identity at boot, so it re-reads the new project from the DB on the next
//! restart.
//!
//! Auth + project header: same `/v1` gate as the rest of setup.

use anseo_core::{project_id_for_name, prompt_id_for, CompetitorConfig, ProviderName};
use anseo_providers::ProviderRequest;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/setup/brand", get(get_brand).put(put_brand))
        .route(
            "/setup/brand/suggest-competitors",
            post(suggest_competitors),
        )
}

#[derive(Debug, Serialize)]
pub struct BrandView {
    pub project_id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub competitors: Vec<CompetitorConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BrandUpdate {
    pub name: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub competitors: Vec<CompetitorConfig>,
    #[serde(default)]
    pub site_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BrandUpdateResult {
    pub project_id: String,
    pub name: String,
    pub variants: Vec<String>,
    pub competitors: Vec<CompetitorConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_url: Option<String>,
    /// `true` when the name changed and thus `project_id` was re-derived; the
    /// operator must restart the API for the new identity to take effect.
    pub restart_required: bool,
}

fn err(code: StatusCode, error: &str, message: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        code,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

fn parse_competitors(v: &serde_json::Value) -> Vec<CompetitorConfig> {
    serde_json::from_value(v.clone()).unwrap_or_default()
}

async fn get_brand(
    project: crate::extractors::ProjectScope,
    State(state): State<AppState>,
) -> Result<Json<BrandView>, (StatusCode, Json<serde_json::Value>)> {
    let project_id = project.id();
    match state.storage.projects().get_brand(project_id).await {
        Ok(Some(row)) => Ok(Json(BrandView {
            project_id: project_id.to_string(),
            name: row.name,
            variants: row.variants,
            competitors: parse_competitors(&row.competitors),
            site_url: row.site_url,
        })),
        // No DB row yet — fall back to the bootstrap YAML so the editor can
        // render the seeded values before the first save.
        Ok(None) => match state.config.as_ref() {
            Some(cfg) => Ok(Json(BrandView {
                project_id: project_id.to_string(),
                name: cfg.brand.name.clone(),
                variants: cfg.brand.variants.clone(),
                competitors: cfg.competitors.clone(),
                site_url: cfg.brand.site_url.clone(),
            })),
            None => Err(err(
                StatusCode::SERVICE_UNAVAILABLE,
                "no_brand_configured",
                "no project row and no anseo.yaml present".to_string(),
            )),
        },
        Err(e) => Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            e.to_string(),
        )),
    }
}

async fn put_brand(
    project: crate::extractors::ProjectScope,
    State(state): State<AppState>,
    Json(body): Json<BrandUpdate>,
) -> Result<Json<BrandUpdateResult>, (StatusCode, Json<serde_json::Value>)> {
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "empty_name",
            "brand name must not be empty".to_string(),
        ));
    }
    let projects = state.storage.projects();
    let current_id = project.id();
    let new_id = project_id_for_name(&name);

    let competitors_json = serde_json::to_value(&body.competitors).map_err(|e| {
        err(
            StatusCode::BAD_REQUEST,
            "invalid_competitors",
            e.to_string(),
        )
    })?;

    // Ensure a project row exists so an edit always has something to write
    // (fresh dev binds may not have seeded one yet).
    let existing = projects.get_brand(current_id).await.map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            e.to_string(),
        )
    })?;
    if existing.is_none() {
        projects
            .insert(&anseo_storage::models::ProjectRow {
                id: current_id,
                name: name.clone(),
                organization_id: None,
                tenant_id: None,
                created_at: chrono::Utc::now(),
            })
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    e.to_string(),
                )
            })?;
    }

    let renamed = new_id != current_id;
    if renamed {
        // Rename re-derives project_id + every prompt_id. Only safe with no
        // prompt_runs — otherwise the run/mention/citation chain would need
        // the deferred full cascade re-key.
        let runs = projects.prompt_run_count(current_id).await.map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                e.to_string(),
            )
        })?;
        if runs > 0 {
            return Err(err(
                StatusCode::CONFLICT,
                "rename_blocked_has_runs",
                format!(
                    "renaming the brand changes project_id and all prompt ids; \
                     this project has {runs} prompt run(s). Re-keying existing runs \
                     is not yet supported — rename is only allowed before the first run."
                ),
            ));
        }
        // Remap every existing prompt id to its new-brand-name derivation.
        let prompts = state
            .storage
            .prompts()
            .list_by_project(current_id)
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    e.to_string(),
                )
            })?;
        let remap: Vec<(anseo_core::PromptId, anseo_core::PromptId)> = prompts
            .iter()
            .map(|p| (p.id, prompt_id_for(&name, &p.name)))
            .collect();
        projects
            .rename_on_empty(
                current_id,
                new_id,
                &name,
                &body.variants,
                &competitors_json,
                body.site_url.as_deref(),
                &remap,
            )
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "rename_failed",
                    e.to_string(),
                )
            })?;
    } else {
        projects
            .update_brand(
                current_id,
                &name,
                &body.variants,
                &competitors_json,
                body.site_url.as_deref(),
            )
            .await
            .map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "storage_error",
                    e.to_string(),
                )
            })?;
    }

    Ok(Json(BrandUpdateResult {
        project_id: if renamed { new_id } else { current_id }.to_string(),
        name,
        variants: body.variants,
        competitors: body.competitors,
        site_url: body.site_url,
        restart_required: renamed,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SuggestRequest {
    /// Wire name of the provider to ask (must have a configured key).
    pub provider: String,
}

#[derive(Debug, Serialize)]
pub struct SuggestResult {
    pub competitors: Vec<CompetitorConfig>,
    pub provider: String,
    pub model: String,
}

/// `POST /v1/setup/brand/suggest-competitors` — ask a configured LLM for a set
/// of likely competitors of the current brand. The operator picks the provider
/// in the UI; we look it up in the boot-loaded registry, send a single
/// completion, and parse a JSON array of names. Suggestions are returned for
/// review — nothing is persisted here; the editor merges + saves via PUT.
async fn suggest_competitors(
    project: crate::extractors::ProjectScope,
    State(state): State<AppState>,
    Json(body): Json<SuggestRequest>,
) -> Result<Json<SuggestResult>, (StatusCode, Json<serde_json::Value>)> {
    let Some(provider_name) = ProviderName::parse(&body.provider) else {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "invalid_provider",
            format!("unknown provider `{}`", body.provider),
        ));
    };
    let provider = crate::routes::provider_lookup::provider_for_request(
        &state,
        &provider_name,
        &body.provider,
    )
    .await?;

    // Read the current brand from the DB (authoritative) so suggestions are
    // grounded in the operator's actual brand + variants.
    let brand_name = project.name().to_string();
    let variants = match state.storage.projects().get_brand(project.id()).await {
        Ok(Some(row)) => row.variants,
        _ => Vec::new(),
    };

    let model = provider_name.default_model().to_string();
    let alias_hint = if variants.is_empty() {
        String::new()
    } else {
        format!(" (also known as: {})", variants.join(", "))
    };
    let prompt = format!(
        "List the main competitors of the brand \"{brand_name}\"{alias_hint}. \
         Respond with ONLY a JSON array of 5 to 8 competitor brand names as strings, \
         e.g. [\"Acme\", \"Globex\"]. No prose, no markdown, no code fences."
    );

    let request = ProviderRequest::new(prompt, model.clone()).with_timeout(Duration::from_secs(30));
    let response = provider.run(request).await.map_err(|e| {
        err(
            StatusCode::BAD_GATEWAY,
            "provider_error",
            format!("provider call failed: {e}"),
        )
    })?;

    let names = parse_competitor_names(&response.message_text)
        .map_err(|m| err(StatusCode::BAD_GATEWAY, "unparseable_suggestion", m))?;

    let competitors = names
        .into_iter()
        .map(|name| CompetitorConfig {
            name,
            variants: Vec::new(),
        })
        .collect();

    Ok(Json(SuggestResult {
        competitors,
        provider: body.provider,
        model,
    }))
}

/// Extract a JSON array of strings from a model response, tolerating stray
/// prose or ```json fences around the array.
fn parse_competitor_names(text: &str) -> Result<Vec<String>, String> {
    let start = text.find('[');
    let end = text.rfind(']');
    let slice = match (start, end) {
        (Some(s), Some(e)) if e > s => &text[s..=e],
        _ => return Err("model response did not contain a JSON array".to_string()),
    };
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(slice).map_err(|e| format!("could not parse JSON array: {e}"))?;
    let names: Vec<String> = raw
        .into_iter()
        .filter_map(|v| match v {
            serde_json::Value::String(s) => Some(s),
            serde_json::Value::Object(o) => o
                .get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string()),
            _ => None,
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if names.is_empty() {
        return Err("model returned an empty competitor list".to_string());
    }
    Ok(names)
}
