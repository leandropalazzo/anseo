//! `/v1/projects` — operator-scoped project registry (Story 36.3, ADR-004).
//!
//! Single-operator multi-project: these endpoints manage the *set* of projects
//! a deployment hosts. They are **project-agnostic** — unlike the rest of
//! `/v1/*`, they are NOT gated by the `X-OpenGEO-Project` header (you can't
//! select a project before you've listed/created one). They remain behind the
//! `require_api_key` auth gate. There is no org/tenant scoping (single
//! operator).
//!
//! - `GET /v1/projects` — list active (non-archived) projects.
//! - `POST /v1/projects` — create from a brand; returns the derived `project_id`.
//! - `GET /v1/projects/{id}` — fetch one project by id.
//! - `POST /v1/projects/{id}/archive` — soft-delete (idempotent).
//!
//! Backed by the Story 36.1 storage methods on `ProjectRepo`.

use std::str::FromStr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use opengeo_core::{BrandConfig, ProjectId};
use opengeo_storage::models::ProjectRow;
use serde::{Deserialize, Serialize};

use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route("/projects/:id", get(get_project))
        .route("/projects/:id/archive", post(archive_project))
}

/// Wire view of a single project row.
#[derive(Debug, Serialize)]
pub struct ProjectView {
    pub project_id: String,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<ProjectRow> for ProjectView {
    fn from(row: ProjectRow) -> Self {
        Self {
            project_id: row.id.to_string(),
            name: row.name,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectListResponse {
    pub projects: Vec<ProjectView>,
}

/// Create a project from a brand. `project_id` is derived from `name` (the same
/// identity the YAML boot path uses), so the operator does not supply it.
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    #[serde(default)]
    pub variants: Vec<String>,
    #[serde(default)]
    pub site_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateProjectResponse {
    pub project_id: String,
    pub name: String,
}

fn err(code: StatusCode, error: &str, message: String) -> (StatusCode, Json<serde_json::Value>) {
    (
        code,
        Json(serde_json::json!({ "error": error, "message": message })),
    )
}

/// Parse a path id into a `ProjectId`, mapping a malformed id to 404 (an
/// undecodable id can never name an existing project).
fn parse_id(raw: &str) -> Result<ProjectId, (StatusCode, Json<serde_json::Value>)> {
    ProjectId::from_str(raw).map_err(|_| {
        err(
            StatusCode::NOT_FOUND,
            "project_not_found",
            format!("`{raw}` is not a valid project id"),
        )
    })
}

async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<ProjectListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let rows = state
        .storage
        .projects()
        .list_projects()
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                e.to_string(),
            )
        })?;
    Ok(Json(ProjectListResponse {
        projects: rows.into_iter().map(ProjectView::from).collect(),
    }))
}

async fn create_project(
    State(state): State<AppState>,
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<CreateProjectResponse>), (StatusCode, Json<serde_json::Value>)> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "project name must not be empty".to_string(),
        ));
    }

    let brand = BrandConfig {
        name: name.clone(),
        variants: req.variants,
        site_url: req.site_url,
    };
    let id = opengeo_core::project_id_for_name(&brand.name);

    // Creating an existing project is a conflict — the id is derived from the
    // name, so two creates with the same name collide deterministically.
    match state.storage.projects().get_project(id).await {
        Ok(Some(_)) => {
            return Err(err(
                StatusCode::CONFLICT,
                "project_exists",
                format!("a project named `{name}` already exists"),
            ));
        }
        Ok(None) => {}
        Err(e) => {
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                e.to_string(),
            ));
        }
    }

    let id = state
        .storage
        .projects()
        .create_project(&brand)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                e.to_string(),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateProjectResponse {
            project_id: id.to_string(),
            name,
        }),
    ))
}

async fn get_project(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
) -> Result<Json<ProjectView>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&raw_id)?;
    match state.storage.projects().get_project(id).await {
        Ok(Some(row)) => Ok(Json(ProjectView::from(row))),
        Ok(None) => Err(err(
            StatusCode::NOT_FOUND,
            "project_not_found",
            format!("no project with id `{raw_id}`"),
        )),
        Err(e) => Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "storage_error",
            e.to_string(),
        )),
    }
}

async fn archive_project(
    State(state): State<AppState>,
    Path(raw_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&raw_id)?;
    // 404 on an unknown id so archive is not a silent no-op against a typo.
    // (archive_project itself is idempotent for an already-archived project.)
    match state.storage.projects().get_project(id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err(err(
                StatusCode::NOT_FOUND,
                "project_not_found",
                format!("no project with id `{raw_id}`"),
            ));
        }
        Err(e) => {
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                e.to_string(),
            ));
        }
    }
    state
        .storage
        .projects()
        .archive_project(id)
        .await
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage_error",
                e.to_string(),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opengeo_core::ProjectId;
    use opengeo_storage::models::ProjectRow;

    #[test]
    fn project_view_from_row_maps_fields() {
        let id = ProjectId::new();
        let now = chrono::Utc::now();
        let row = ProjectRow {
            id,
            name: "Acme Corp".to_string(),
            organization_id: None,
            tenant_id: None,
            created_at: now,
        };
        let view = ProjectView::from(row);
        assert_eq!(view.project_id, id.to_string());
        assert_eq!(view.name, "Acme Corp");
        assert_eq!(view.created_at, now);
    }

    #[test]
    fn parse_id_rejects_garbage() {
        let result = parse_id("not-a-valid-id");
        assert!(result.is_err());
        let (code, _) = result.unwrap_err();
        assert_eq!(code, StatusCode::NOT_FOUND);
    }

    #[test]
    fn create_project_request_deserializes_defaults() {
        let json = r#"{"name": "My Brand"}"#;
        let req: CreateProjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "My Brand");
        assert!(req.variants.is_empty());
        assert!(req.site_url.is_none());
    }
}
