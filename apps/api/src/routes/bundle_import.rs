//! Story 27.8 — Hosted import + tenant backfill.
//!
//! POST /v1/orgs/:org_id/import/bundle
//!     Accepts a gzip-compressed JSON bundle (produced by `ogeo export bundle`)
//!     and idempotently inserts projects, prompts, and prompt runs into the
//!     target org. Provider keys are NEVER restored from the bundle (AC-3).
//!
//! Design notes:
//!   AC-1: Stamps org_id on every inserted row; preserves original timestamps.
//!   AC-2: Import is idempotent (INSERT … ON CONFLICT DO NOTHING by id).
//!   AC-3: Provider keys are absent from the bundle format by construction.

use anseo_authz::matrix::Capability;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use uuid::Uuid;

use crate::middleware::authz::{enforce_capability, RequiredCapability};
use crate::middleware::org_guc::OrgContext;
use crate::AppState;

pub fn v1_router() -> Router<AppState> {
    Router::new().route(
        "/orgs/:org_id/import/bundle",
        post(import_bundle).layer(Extension(RequiredCapability(Capability::OrgRead))),
    )
}

/// Mirrors the bundle format from `ogeo export bundle` (Story 27.7).
#[derive(Debug, Deserialize)]
struct Bundle {
    bundle_version: String,
    #[allow(dead_code)]
    exported_at: DateTime<Utc>,
    projects: Vec<BundleProject>,
}

#[derive(Debug, Deserialize)]
struct BundleProject {
    id: String,
    name: String,
    created_at: DateTime<Utc>,
    prompts: Vec<BundlePrompt>,
}

#[derive(Debug, Deserialize)]
struct BundlePrompt {
    id: String,
    name: String,
    text: String,
    tags: Vec<String>,
    created_at: DateTime<Utc>,
    runs: Vec<BundleRun>,
}

#[derive(Debug, Deserialize)]
struct BundleRun {
    id: String,
    provider: String,
    provider_model_version: String,
    status: String,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub bundle_version: String,
    pub projects_imported: usize,
    pub prompts_imported: usize,
    pub runs_imported: usize,
    pub skipped_duplicate_ids: usize,
}

async fn import_bundle(
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    org_context: Option<Extension<OrgContext>>,
    body: Bytes,
) -> Result<(StatusCode, Json<ImportSummary>), (StatusCode, Json<serde_json::Value>)> {
    enforce_capability(&state, org_context.map(|Extension(ctx)| ctx), Capability::OrgRead)
        .await
        .map_err(|r| {
            let status = r.status();
            (status, Json(serde_json::json!({"error": "forbidden"})))
        })?;

    // Decompress.
    let mut decoder = GzDecoder::new(body.as_ref());
    let mut json_bytes = Vec::new();
    decoder.read_to_end(&mut json_bytes).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": format!("decompression failed: {e}")})),
        )
    })?;

    let bundle: Bundle = serde_json::from_slice(&json_bytes).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": format!("invalid bundle: {e}")})),
        )
    })?;

    // Reject unknown major versions.
    if !bundle.bundle_version.starts_with("1.") {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": format!("unsupported bundle_version: {}", bundle.bundle_version)})),
        ));
    }

    let pool = state.storage.pool();
    let mut projects_imported = 0usize;
    let mut prompts_imported = 0usize;
    let mut runs_imported = 0usize;
    let mut skipped = 0usize;

    for proj in &bundle.projects {
        let proj_uuid = proj.id.parse::<Uuid>().map_err(|_| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({"error": format!("invalid project id: {}", proj.id)})),
            )
        })?;

        // Idempotent insert — ON CONFLICT DO NOTHING by natural PK.
        let inserted = sqlx::query(
            r#"
            INSERT INTO projects (id, name, organization_id, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (id) DO NOTHING
            "#,
        )
        .bind(proj_uuid)
        .bind(&proj.name)
        .bind(org_id)
        .bind(proj.created_at)
        .execute(pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("db error: {e}")})),
            )
        })?;

        if inserted.rows_affected() == 0 {
            skipped += 1;
        } else {
            projects_imported += 1;
        }

        for prompt in &proj.prompts {
            let prompt_uuid = prompt.id.parse::<Uuid>().map_err(|_| {
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({"error": format!("invalid prompt id: {}", prompt.id)})),
                )
            })?;

            let tags_json = serde_json::to_value(&prompt.tags).unwrap_or_default();
            let p_inserted = sqlx::query(
                r#"
                INSERT INTO prompts (id, project_id, name, text, tags, organization_id, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ON CONFLICT (id) DO NOTHING
                "#,
            )
            .bind(prompt_uuid)
            .bind(proj_uuid)
            .bind(&prompt.name)
            .bind(&prompt.text)
            .bind(&tags_json)
            .bind(org_id)
            .bind(prompt.created_at)
            .execute(pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("db error: {e}")})),
                )
            })?;

            if p_inserted.rows_affected() == 0 {
                skipped += 1;
            } else {
                prompts_imported += 1;
            }

            for run in &prompt.runs {
                let run_uuid = run.id.parse::<Uuid>().map_err(|_| {
                    (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(serde_json::json!({"error": format!("invalid run id: {}", run.id)})),
                    )
                })?;

                let r_inserted = sqlx::query(
                    r#"
                    INSERT INTO prompt_runs
                        (id, prompt_id, provider, provider_model_version,
                         status, started_at, finished_at,
                         raw_response, request_parameters,
                         organization_id, created_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, '{}', '{}', $8, $9)
                    ON CONFLICT (id) DO NOTHING
                    "#,
                )
                .bind(run_uuid)
                .bind(prompt_uuid)
                .bind(&run.provider)
                .bind(&run.provider_model_version)
                .bind(&run.status)
                .bind(run.started_at)
                .bind(run.finished_at)
                .bind(org_id)
                .bind(run.created_at)
                .execute(pool)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": format!("db error: {e}")})),
                    )
                })?;

                if r_inserted.rows_affected() == 0 {
                    skipped += 1;
                } else {
                    runs_imported += 1;
                }
            }
        }
    }

    Ok((
        StatusCode::OK,
        Json(ImportSummary {
            bundle_version: bundle.bundle_version,
            projects_imported,
            prompts_imported,
            runs_imported,
            skipped_duplicate_ids: skipped,
        }),
    ))
}
