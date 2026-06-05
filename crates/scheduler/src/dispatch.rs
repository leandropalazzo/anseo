//! Schedule tick discovery + dispatch (FR-26, the Story 10.4 follow-up that
//! `apps/worker/src/main.rs` left unimplemented).
//!
//! Each poll the worker calls [`dispatch_due_schedules`], which:
//!   1. discovers non-paused schedules whose anchored next tick is due,
//!   2. claims each due tick at-most-once via [`crate::worker::claim_tick`],
//!   3. runs the schedule's `prompts × providers` matrix through the live
//!      [`Orchestrator`], persisting one `prompt_runs` row per cell linked to
//!      the owning `schedule_tick_id`,
//!   4. marks the tick `completed` (or `failed`) and returns the lifecycle
//!      events the worker publishes over Postgres NOTIFY + webhook fanout.
//!
//! ## Why a per-tick `Config` is rebuilt
//!
//! The [`Orchestrator`] expands its matrix from `config.prompts ×
//! config.providers`. Prompts are DB-authoritative (operators create them via
//! the dashboard, not `opengeo.yaml`), and a schedule names its own provider
//! set, so neither is guaranteed to be in the worker's boot YAML. We therefore
//! synthesise a `Config` whose `prompts` are the schedule's DB prompt rows and
//! whose `providers` are the schedule's configured providers (those present in
//! the registry). The base config still supplies brand identity + concurrency.

use anseo_core::{Config, ProjectId, PromptConfig, ProviderConfig, ProviderName};
use anseo_providers::{Orchestrator, OrchestratorFilter, PromptRunStatus, ProviderRegistry};
use anseo_storage::Storage;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::events::{CompletedPayload, FailedPayload, LifecycleEvent};
use crate::worker::{claim_tick, next_tick_for, payload_for, ClaimOutcome, WorkerError};

/// A schedule whose next anchored tick has come due.
#[derive(Debug, Clone, PartialEq)]
pub struct DueSchedule {
    pub schedule_id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub cron: String,
    pub prompts: Vec<String>,
    pub providers: Vec<String>,
    /// The anchored tick boundary being claimed (not `now`).
    pub tick_ts: DateTime<Utc>,
}

/// Discover non-paused schedules whose next anchored tick is `<= now`.
///
/// The anchor is the schedule's most recent `schedule_ticks.tick_ts` (any
/// status), falling back to `created_at` for a schedule that has never ticked.
/// Schedules with an unparseable cadence are skipped (they could never have
/// passed create-time validation, but a manual DB edit shouldn't wedge the
/// loop).
pub async fn discover_due_schedules(
    pool: &PgPool,
    now: DateTime<Utc>,
) -> Result<Vec<DueSchedule>, WorkerError> {
    discover_due_schedules_scoped(pool, None, now).await
}

/// Project-scoped variant of [`discover_due_schedules`]. When `project_id` is
/// `Some`, only schedules owned by that project are considered — this is the
/// seam Epic 36's worker fan-out uses so each project's due ticks dispatch
/// against that project's own brand-overlaid config. `None` preserves the
/// global behaviour.
pub async fn discover_due_schedules_scoped(
    pool: &PgPool,
    project_id: Option<Uuid>,
    now: DateTime<Utc>,
) -> Result<Vec<DueSchedule>, WorkerError> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id          AS id,
            s.project_id  AS project_id,
            s.name        AS name,
            s.cron        AS cron,
            s.prompts     AS prompts,
            s.providers   AS providers,
            s.created_at  AS created_at,
            (
                SELECT MAX(t.tick_ts)
                FROM schedule_ticks t
                WHERE t.schedule_id = s.id
            ) AS last_tick_ts
        FROM schedules s
        WHERE s.paused = FALSE
          AND ($1::uuid IS NULL OR s.project_id = $1)
        "#,
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;

    let mut due = Vec::new();
    for r in rows {
        let cron: String = r.try_get("cron")?;
        let created_at: DateTime<Utc> = r.try_get("created_at")?;
        let last_tick_ts: Option<DateTime<Utc>> = r.try_get("last_tick_ts")?;
        let anchor = last_tick_ts.unwrap_or(created_at);
        let Ok(next_tick) = next_tick_for(&cron, anchor) else {
            tracing::warn!(
                schedule = %r.try_get::<String, _>("name").unwrap_or_default(),
                cron = %cron,
                "skipping schedule with unparseable cadence"
            );
            continue;
        };
        if next_tick > now {
            continue;
        }
        let prompts_json: serde_json::Value = r.try_get("prompts")?;
        let providers_json: serde_json::Value = r.try_get("providers")?;
        due.push(DueSchedule {
            schedule_id: r.try_get("id")?,
            project_id: r.try_get("project_id")?,
            name: r.try_get("name")?,
            cron,
            prompts: serde_json::from_value(prompts_json).unwrap_or_default(),
            providers: serde_json::from_value(providers_json).unwrap_or_default(),
            tick_ts: next_tick,
        });
    }
    Ok(due)
}

/// Discover + dispatch every due tick. Returns the lifecycle events the caller
/// must publish (NOTIFY) and fan out to webhooks — mirrors the reaper contract
/// in `apps/worker/src/main.rs`, which owns the transport side-effects.
pub async fn dispatch_due_schedules(
    pool: &PgPool,
    storage: &Storage,
    base_config: &Config,
    registry: &ProviderRegistry,
    claimed_by: &str,
    now: DateTime<Utc>,
) -> Result<Vec<LifecycleEvent>, WorkerError> {
    dispatch_due_schedules_scoped(pool, storage, base_config, registry, None, claimed_by, now).await
}

/// Project-scoped variant of [`dispatch_due_schedules`]. When `project_id` is
/// `Some`, only that project's due ticks are discovered, claimed, and run —
/// so the caller can pass a `base_config` whose brand overlay matches the
/// project. Epic 36's worker fan-out calls this once per active project each
/// tick. `None` preserves the global behaviour.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_due_schedules_scoped(
    pool: &PgPool,
    storage: &Storage,
    base_config: &Config,
    registry: &ProviderRegistry,
    project_id: Option<Uuid>,
    claimed_by: &str,
    now: DateTime<Utc>,
) -> Result<Vec<LifecycleEvent>, WorkerError> {
    let due = discover_due_schedules_scoped(pool, project_id, now).await?;
    let mut events = Vec::new();
    for d in due {
        let ClaimOutcome::Claimed { tick_id } =
            claim_tick(pool, d.schedule_id, d.tick_ts, claimed_by).await?
        else {
            // Another worker (or an earlier poll this same slot) already owns
            // this tick. Not an error; just skip.
            continue;
        };

        let base = payload_for(d.project_id, d.schedule_id, &d.name, tick_id, d.tick_ts);
        events.push(LifecycleEvent::TickClaimed(base.clone()));

        match run_tick(storage, base_config, registry, &d, tick_id).await {
            Ok((total, failed)) => {
                mark_tick(pool, tick_id, "completed", None).await?;
                tracing::info!(
                    event = "schedule.tick_completed",
                    schedule = %d.name,
                    prompt_run_count = total,
                    failed_run_count = failed,
                    "dispatched scheduled tick"
                );
                events.push(LifecycleEvent::TickCompleted(CompletedPayload {
                    base,
                    prompt_run_count: total,
                    failed_run_count: failed,
                }));
            }
            Err(err) => {
                let msg = err.to_string();
                mark_tick(pool, tick_id, "failed", Some(&msg)).await?;
                tracing::warn!(
                    event = "schedule.tick_failed",
                    schedule = %d.name,
                    error = %msg,
                    "scheduled tick dispatch failed"
                );
                events.push(LifecycleEvent::TickFailed(FailedPayload {
                    base,
                    error_message: msg,
                }));
            }
        }
    }
    Ok(events)
}

/// Outcome of a manual [`run_schedule_now`] trigger.
#[derive(Debug, Clone, PartialEq)]
pub struct RunNowOutcome {
    pub tick_id: Uuid,
    pub prompt_run_count: u32,
    pub failed_run_count: u32,
}

/// Manually dispatch a single schedule immediately, ignoring its cadence and
/// `paused` flag. Claims a tick at `now` (sub-second precision keeps it from
/// colliding with the cadence-aligned ticks the worker claims), runs the
/// schedule's `prompts × providers` matrix through the orchestrator, persists
/// the runs linked to the new `schedule_tick_id`, and marks the tick terminal.
///
/// Returns `Ok(None)` when no schedule with `schedule_id` exists in
/// `project_id` (the caller should surface a 404).
#[allow(clippy::too_many_arguments)]
pub async fn run_schedule_now(
    pool: &PgPool,
    storage: &Storage,
    base_config: &Config,
    registry: &ProviderRegistry,
    schedule_id: Uuid,
    project_id: Uuid,
    claimed_by: &str,
    now: DateTime<Utc>,
) -> Result<Option<RunNowOutcome>, WorkerError> {
    let Some(row) = sqlx::query(
        r#"
        SELECT name, cron, prompts, providers
        FROM schedules
        WHERE id = $1 AND project_id = $2
        "#,
    )
    .bind(schedule_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let prompts_json: serde_json::Value = row.try_get("prompts")?;
    let providers_json: serde_json::Value = row.try_get("providers")?;
    let due = DueSchedule {
        schedule_id,
        project_id,
        name: row.try_get("name")?,
        cron: row.try_get("cron")?,
        prompts: serde_json::from_value(prompts_json).unwrap_or_default(),
        providers: serde_json::from_value(providers_json).unwrap_or_default(),
        tick_ts: now,
    };

    let ClaimOutcome::Claimed { tick_id } = claim_tick(pool, schedule_id, now, claimed_by).await?
    else {
        // A tick already exists at this exact instant (e.g. a double-click);
        // treat as a no-op rather than running twice.
        return Err(WorkerError::TickAlreadyClaimed);
    };

    match run_tick(storage, base_config, registry, &due, tick_id).await {
        Ok((total, failed)) => {
            mark_tick(pool, tick_id, "completed", None).await?;
            tracing::info!(
                event = "schedule.run_now_completed",
                schedule = %due.name,
                prompt_run_count = total,
                failed_run_count = failed,
                "manual schedule run completed"
            );
            Ok(Some(RunNowOutcome {
                tick_id,
                prompt_run_count: total,
                failed_run_count: failed,
            }))
        }
        Err(e) => {
            let msg = e.to_string();
            mark_tick(pool, tick_id, "failed", Some(&msg)).await?;
            Err(e)
        }
    }
}

/// Run one claimed tick: build the per-tick config, drive the orchestrator,
/// persist a `prompt_runs` row per cell linked to `tick_id`. Returns
/// `(total_runs, failed_runs)`.
async fn run_tick(
    storage: &Storage,
    base_config: &Config,
    registry: &ProviderRegistry,
    schedule: &DueSchedule,
    tick_id: Uuid,
) -> Result<(u32, u32), WorkerError> {
    let project_id = ProjectId::from_ulid(ulid::Ulid::from_bytes(*schedule.project_id.as_bytes()));

    // DB-authoritative prompts: resolve each scheduled prompt name to its
    // current row so the persisted run links to the real FK prompt_id and uses
    // the latest prompt text.
    let prompt_repo = storage.prompts();
    let mut prompt_configs = Vec::new();
    let mut prompt_ids = std::collections::HashMap::new();
    for name in &schedule.prompts {
        let Some(row) = prompt_repo.find_by_name(project_id, name).await? else {
            tracing::warn!(
                schedule = %schedule.name,
                prompt = %name,
                "scheduled prompt not found in DB; skipping cell"
            );
            continue;
        };
        prompt_configs.push(PromptConfig {
            name: row.name.clone(),
            text: row.text.clone(),
            description: None,
        });
        prompt_ids.insert(row.name.clone(), row.id);
    }
    if prompt_configs.is_empty() {
        return Ok((0, 0));
    }

    // Providers: the schedule's declared set, filtered to those that are
    // actually configured (present in the registry). Unconfigured providers
    // are dropped rather than synthesising failed rows on every tick.
    //
    // OpenRouter entries are special: a bare `"openrouter"` means auto-routing
    // (one run, upstream model chosen by OpenRouter), while `"openrouter:<vendor>/<model>"`
    // pins a specific upstream model. Multiple pinned entries collapse into a
    // single OpenRouter `ProviderConfig` with a `models` list, which the
    // orchestrator fans out into one run per model.
    let timeout = anseo_core::config::DEFAULT_PROVIDER_TIMEOUT_SECONDS;
    let mut providers: Vec<ProviderConfig> = Vec::new();
    let mut openrouter_models: Vec<String> = Vec::new();
    let mut openrouter_auto = false;
    for entry in &schedule.providers {
        if let Some(model) = entry.strip_prefix("openrouter:") {
            let model = model.trim();
            if !model.is_empty() && !openrouter_models.iter().any(|m| m == model) {
                openrouter_models.push(model.to_string());
            }
            continue;
        }
        let Some(name) = ProviderName::parse(entry) else {
            continue;
        };
        if name == ProviderName::Openrouter {
            openrouter_auto = true;
            continue;
        }
        if registry.contains_key(&name) {
            providers.push(ProviderConfig {
                name,
                model: None,
                models: None,
                timeout_seconds: timeout,
            });
        }
    }
    if (openrouter_auto || !openrouter_models.is_empty())
        && registry.contains_key(&ProviderName::Openrouter)
    {
        providers.push(ProviderConfig {
            name: ProviderName::Openrouter,
            model: None,
            models: if openrouter_models.is_empty() {
                None
            } else {
                Some(openrouter_models)
            },
            timeout_seconds: timeout,
        });
    }
    if providers.is_empty() {
        return Ok((0, 0));
    }

    let mut run_config = base_config.clone();
    run_config.prompts = prompt_configs;
    run_config.providers = providers;

    let orchestrator = Orchestrator::new(run_config, registry.clone());
    let records = orchestrator.run_all(OrchestratorFilter::default()).await;

    let now = Utc::now();
    let mut total = 0u32;
    let mut failed = 0u32;
    for record in records {
        let Some(prompt_id) = prompt_ids.get(&record.prompt_name).copied() else {
            continue;
        };
        if record.status == PromptRunStatus::Failed {
            failed += 1;
        }
        let mut params = record.request_parameters.clone();
        if let Some(obj) = params.as_object_mut() {
            obj.insert("triggered_by".to_string(), serde_json::json!("schedule"));
        }
        let provider_wire = record.provider.as_wire_str();
        let error_kind = record.error_kind.map(|k| k.as_wire_str().to_string());
        let run_id = record.id;
        let message_text = record.message_text.clone();
        insert_scheduled_run(
            storage.pool(),
            Uuid::from_bytes(run_id.into_ulid().to_bytes()),
            Uuid::from_bytes(prompt_id.into_ulid().to_bytes()),
            provider_wire.as_ref(),
            &record.provider_model_version,
            record.provider_region.as_deref(),
            record.started_at,
            record.finished_at,
            &record.raw_response,
            &params,
            record.status.as_wire_str(),
            error_kind.as_deref(),
            tick_id,
            now,
        )
        .await?;
        total += 1;

        // Parse the response into mentions + citations so the analytics
        // surfaces (brand rank, visibility, share of voice) have data. The
        // base config carries the DB brand overlay (brand name + competitors).
        if let Some(text) = message_text.as_deref() {
            if let Err(e) = anseo_extractors::extract_and_persist(
                storage,
                base_config,
                run_id,
                text,
                &record.raw_response,
                now,
            )
            .await
            {
                tracing::warn!(
                    schedule = %schedule.name,
                    error = %e,
                    "mention/citation extraction failed for scheduled run"
                );
            }
        }
    }
    Ok((total, failed))
}

/// INSERT one scheduled `prompt_runs` row with its `schedule_tick_id` set.
/// Runtime query (not the compile-time macro) so the offline `.sqlx/` cache
/// stays untouched — matching the repo conventions in `crates/storage`.
#[allow(clippy::too_many_arguments)]
async fn insert_scheduled_run(
    pool: &PgPool,
    run_id: Uuid,
    prompt_id: Uuid,
    provider: &str,
    provider_model_version: &str,
    provider_region: Option<&str>,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    raw_response: &serde_json::Value,
    request_parameters: &serde_json::Value,
    status: &str,
    error_kind: Option<&str>,
    schedule_tick_id: Uuid,
    created_at: DateTime<Utc>,
) -> Result<(), WorkerError> {
    sqlx::query(
        r#"
        INSERT INTO prompt_runs (
            id, prompt_id, provider, provider_model_version, provider_region,
            started_at, finished_at, raw_response, request_parameters,
            status, error_kind, schedule_tick_id, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(run_id)
    .bind(prompt_id)
    .bind(provider)
    .bind(provider_model_version)
    .bind(provider_region)
    .bind(started_at)
    .bind(finished_at)
    .bind(raw_response)
    .bind(request_parameters)
    .bind(status)
    .bind(error_kind)
    .bind(schedule_tick_id)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Transition a claimed tick to its terminal status.
async fn mark_tick(
    pool: &PgPool,
    tick_id: Uuid,
    status: &str,
    error_message: Option<&str>,
) -> Result<(), WorkerError> {
    sqlx::query(
        r#"
        UPDATE schedule_ticks
        SET status = $2, completed_at = now(), error_message = $3
        WHERE id = $1
        "#,
    )
    .bind(tick_id)
    .bind(status)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}
