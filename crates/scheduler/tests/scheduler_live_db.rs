//! Phase 2 Story 10.2 — live-Postgres integration tests for the
//! scheduler's architectural promises (P0-102, P0-103, P0-104, ARCH-21).
//!
//! Gated behind the `live_db_tests` Cargo feature.

#![cfg(feature = "live_db_tests")]

use chrono::{TimeZone, Utc};
use opengeo_core::ProjectId;
use opengeo_scheduler::events::{LifecycleEvent, SchedulePayload};
use opengeo_scheduler::transport::{listen, publish};
use opengeo_scheduler::worker::{
    anchor_next_tick, claim_tick, event_channel, reap_orphans, ClaimOutcome, REAPER_IDLE_SECONDS,
};
use opengeo_scheduler::{parse_cadence, ScheduleValidationError};
use opengeo_storage::Storage;
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

async fn fresh_storage() -> Storage {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let storage = Storage::connect(&url).await.expect("connect");
    storage.migrate().await.expect("migrate");
    storage
}

async fn seed_schedule(pool: &PgPool) -> (Uuid, Uuid) {
    let project_id = ProjectId::new();
    let project_uuid = Uuid::from_bytes(project_id.into_ulid().to_bytes());
    sqlx::query("INSERT INTO projects (id, name) VALUES ($1, $2)")
        .bind(project_uuid)
        .bind(format!("sched-test-{}", project_uuid))
        .execute(pool)
        .await
        .expect("project");

    let schedule_id = Uuid::from_u128(ulid::Ulid::new().0);
    sqlx::query(
        r#"INSERT INTO schedules
           (id, project_id, name, cron, prompts, providers)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(schedule_id)
    .bind(project_uuid)
    .bind(format!("sched-{schedule_id}"))
    .bind("hourly")
    .bind(serde_json::json!(["prompt-a"]))
    .bind(serde_json::json!(["openai"]))
    .execute(pool)
    .await
    .expect("schedule");

    (project_uuid, schedule_id)
}

#[tokio::test]
#[serial_test::serial]
async fn p0_102_at_most_once_two_workers_race_same_tick() {
    // ARCH-21 / R-100 (score 9): when two workers race the same
    // (schedule_id, tick_ts), exactly one wins the claim. The unique
    // constraint on schedule_ticks (schedule_id, tick_ts) realizes the
    // at-most-once invariant in the storage layer.
    let storage = fresh_storage().await;
    let pool = storage.pool().clone();
    let (_project, schedule_id) = seed_schedule(&pool).await;
    let tick_ts = Utc.with_ymd_and_hms(2026, 6, 15, 8, 0, 0).unwrap();

    // Spawn two concurrent claims with distinct claimed_by values.
    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let handle_a =
        tokio::spawn(async move { claim_tick(&pool_a, schedule_id, tick_ts, "worker-a").await });
    let handle_b =
        tokio::spawn(async move { claim_tick(&pool_b, schedule_id, tick_ts, "worker-b").await });

    let outcome_a = handle_a
        .await
        .expect("worker-a join")
        .expect("claim_a result");
    let outcome_b = handle_b
        .await
        .expect("worker-b join")
        .expect("claim_b result");

    // Exactly one Claimed, exactly one AlreadyClaimed.
    let claimed_count = [&outcome_a, &outcome_b]
        .iter()
        .filter(|o| matches!(o, ClaimOutcome::Claimed { .. }))
        .count();
    let already_count = [&outcome_a, &outcome_b]
        .iter()
        .filter(|o| matches!(o, ClaimOutcome::AlreadyClaimed))
        .count();
    assert_eq!(claimed_count, 1, "exactly one worker must win the race");
    assert_eq!(already_count, 1, "exactly one worker must lose the race");

    // Exactly one row in schedule_ticks for this (schedule, tick_ts).
    let row_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM schedule_ticks WHERE schedule_id = $1 AND tick_ts = $2"#,
    )
    .bind(schedule_id)
    .bind(tick_ts)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(row_count, 1, "exactly one persisted row for the tick");

    // Row status is 'claimed', claimed_by is one of the workers.
    let (status, claimed_by): (String, String) = sqlx::query_as(
        r#"SELECT status, claimed_by FROM schedule_ticks
           WHERE schedule_id = $1 AND tick_ts = $2"#,
    )
    .bind(schedule_id)
    .bind(tick_ts)
    .fetch_one(&pool)
    .await
    .expect("row fetch");
    assert_eq!(status, "claimed");
    assert!(
        claimed_by == "worker-a" || claimed_by == "worker-b",
        "claimed_by must be one of the racers, got `{claimed_by}`"
    );
}

#[tokio::test]
#[serial_test::serial]
async fn p0_103_rollforward_reaper_marks_orphan_with_full_payload() {
    // ARCH-21: a tick that's been `claimed` for > REAPER_IDLE_SECONDS
    // (5 minutes) without `completed_at` is marked `rolled_forward`.
    // The CTE-backed reaper returns project_id + schedule_name in the
    // payload so the worker can fan out a complete event.
    let storage = fresh_storage().await;
    let pool = storage.pool().clone();
    let (project_uuid, schedule_id) = seed_schedule(&pool).await;

    // Insert a claimed tick with claimed_at 6 minutes ago (past the
    // 5-minute reap window).
    let tick_id = Uuid::from_u128(ulid::Ulid::new().0);
    let tick_ts = Utc.with_ymd_and_hms(2026, 6, 15, 8, 0, 0).unwrap();
    let claimed_at = Utc::now() - chrono::Duration::seconds(REAPER_IDLE_SECONDS + 60);
    sqlx::query(
        r#"INSERT INTO schedule_ticks
           (id, schedule_id, tick_ts, status, claimed_by, claimed_at)
           VALUES ($1, $2, $3, 'claimed', $4, $5)"#,
    )
    .bind(tick_id)
    .bind(schedule_id)
    .bind(tick_ts)
    .bind("worker-crashed")
    .bind(claimed_at)
    .execute(&pool)
    .await
    .expect("insert orphaned tick");

    // Run the reaper.
    let reaped = reap_orphans(&pool).await.expect("reap_orphans");
    assert_eq!(reaped.len(), 1, "reaper must see exactly one orphan");
    let ticket = &reaped[0];
    assert_eq!(ticket.tick_id, tick_id);
    assert_eq!(ticket.schedule_id, schedule_id);
    assert_eq!(
        ticket.project_id, project_uuid,
        "CTE JOIN must populate project_id"
    );
    assert!(
        !ticket.schedule_name.is_empty(),
        "schedule_name must be populated"
    );

    // Row status flipped to rolled_forward.
    let status: String = sqlx::query_scalar(r#"SELECT status FROM schedule_ticks WHERE id = $1"#)
        .bind(tick_id)
        .fetch_one(&pool)
        .await
        .expect("status fetch");
    assert_eq!(status, "rolled_forward");
}

#[tokio::test]
#[serial_test::serial]
async fn p0_103_reaper_leaves_fresh_claims_alone() {
    // Negative case: a tick claimed seconds ago (within the window)
    // must NOT be reaped. Boundary check against the 5-minute cutoff.
    let storage = fresh_storage().await;
    let pool = storage.pool().clone();
    let (_project, schedule_id) = seed_schedule(&pool).await;

    let tick_id = Uuid::from_u128(ulid::Ulid::new().0);
    let tick_ts = Utc.with_ymd_and_hms(2026, 6, 15, 9, 0, 0).unwrap();
    sqlx::query(
        r#"INSERT INTO schedule_ticks
           (id, schedule_id, tick_ts, status, claimed_by, claimed_at)
           VALUES ($1, $2, $3, 'claimed', $4, now())"#,
    )
    .bind(tick_id)
    .bind(schedule_id)
    .bind(tick_ts)
    .bind("worker-busy")
    .execute(&pool)
    .await
    .expect("fresh claim");

    let reaped = reap_orphans(&pool).await.expect("reap");
    assert!(
        !reaped.iter().any(|r| r.tick_id == tick_id),
        "fresh claim must not be reaped"
    );

    let status: String = sqlx::query_scalar(r#"SELECT status FROM schedule_ticks WHERE id = $1"#)
        .bind(tick_id)
        .fetch_one(&pool)
        .await
        .expect("status");
    assert_eq!(status, "claimed");
}

#[tokio::test]
async fn p0_104_anchor_next_tick_uses_epoch_aligned_slot() {
    // Pure-function pin (no DB needed) — kept in the live-DB suite so
    // the architecture's anchored-scheduling promise sits with the
    // other ARCH-21 invariants.
    let last_run = Utc.with_ymd_and_hms(2026, 6, 15, 8, 23, 0).unwrap();
    let cadence = parse_cadence("hourly").expect("parse");
    let next = anchor_next_tick(cadence, last_run);
    // Next hour boundary after 08:23 is 09:00.
    assert_eq!(next, Utc.with_ymd_and_hms(2026, 6, 15, 9, 0, 0).unwrap());

    let daily = parse_cadence("daily").expect("parse");
    let next = anchor_next_tick(daily, last_run);
    assert_eq!(next, Utc.with_ymd_and_hms(2026, 6, 16, 0, 0, 0).unwrap());

    let invalid: Result<_, ScheduleValidationError> = parse_cadence("never");
    assert!(invalid.is_err());
}

#[tokio::test]
#[serial_test::serial]
async fn arch_16_listen_notify_round_trip_publishes_lifecycle_event() {
    // The cross-process transport: worker publishes via pg_notify, API
    // listener forwards into the broadcast channel. Verifies that
    // round-trip end-to-end. Without this guarantee, the SSE endpoint
    // would be silent in production despite the unit tests passing.
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL required");
    let storage = Storage::connect(&url).await.expect("connect");
    storage.migrate().await.expect("migrate");

    let (tx, mut rx) = event_channel();
    let url_clone = url.clone();
    let listener_handle = tokio::spawn(async move {
        // listen() loops forever; we abort when the test is done.
        let _ = listen(&url_clone, tx).await;
    });

    // Give the listener a moment to attach.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish a sample event.
    let event = LifecycleEvent::Missed(SchedulePayload {
        event_id: Uuid::nil(),
        project_id: Uuid::nil(),
        schedule_id: Uuid::nil(),
        schedule_name: "missed-test".into(),
        tick_id: Uuid::nil(),
        tick_ts: Utc::now(),
        emitted_at: Utc::now(),
    });
    publish(storage.pool(), &event).await.expect("publish");

    // Consumer receives the event from the broadcast channel.
    let received = tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("event must arrive within 3s")
        .expect("broadcast channel must still be open");

    assert_eq!(received.kind(), "schedule.missed");
    match received {
        LifecycleEvent::Missed(payload) => assert_eq!(payload.schedule_name, "missed-test"),
        other => panic!("expected Missed, got {other:?}"),
    }

    listener_handle.abort();
    let _ = listener_handle.await;
    // Drop storage to close the pool.
    drop(storage);
}
