//! Background-worker substrate for FR-26 / ARCH-21.
//!
//! At-most-once via a unique-constraint claim on `schedule_ticks
//! (schedule_id, tick_ts)`. Two workers racing the same tick: one
//! `INSERT` succeeds, the other returns zero rows; the loser exits the tick
//! without invoking any Provider. Orphan reaper marks `claimed` rows older
//! than `REAPER_IDLE_SECONDS` as `rolled_forward` so the next anchored tick
//! can run; missed ticks are NOT retried (rollforward, not retry — I-14).
//!
//! Anchored next-tick: computed against the cadence (e.g., `daily` → next
//! UTC midnight; `every N hours` → next N-aligned boundary), not
//! `now + interval`. This keeps schedules aligned across worker restarts
//! and across debounce windows.

use crate::events::{LifecycleEvent, SchedulePayload};
use crate::{
    parse_recurrence, Cadence, CalendarCadence, CalendarSpec, Recurrence, ScheduleValidationError,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use sqlx::{PgPool, Row};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Workers older than this without a `completed_at` are reaped to
/// `rolled_forward`. Matches the 5-minute debounce window so a slow Provider
/// call cannot accidentally race a reap.
pub const REAPER_IDLE_SECONDS: i64 = 300;

/// Default broadcast channel capacity. Subscribers (SSE clients, webhook
/// dispatcher, notification channels) that fall behind drop the oldest event.
pub const EVENT_CHANNEL_CAPACITY: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("storage error")]
    Storage(#[from] anseo_storage::Error),
    #[error("invalid schedule cadence")]
    InvalidCadence(#[from] ScheduleValidationError),
    #[error("a tick for this instant is already claimed")]
    TickAlreadyClaimed,
}

/// Outcome of a single `claim_tick` attempt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClaimOutcome {
    /// This worker won the race; the returned `tick_id` is the row to operate
    /// on. Caller MUST emit `tick_claimed` and proceed to run the tick.
    Claimed { tick_id: Uuid },
    /// Another worker already inserted the row. Caller MUST NOT run the tick.
    AlreadyClaimed,
}

/// At-most-once claim against `schedule_ticks (schedule_id, tick_ts)`.
///
/// Returns `Claimed` when this worker inserted the row, `AlreadyClaimed` when
/// the unique constraint rejected the insert. Never panics; never retries.
pub async fn claim_tick(
    pool: &PgPool,
    schedule_id: Uuid,
    tick_ts: DateTime<Utc>,
    claimed_by: &str,
) -> Result<ClaimOutcome, WorkerError> {
    let tick_id = Uuid::from_u128(ulid::Ulid::new().0);
    let row = sqlx::query(
        r#"
        INSERT INTO schedule_ticks
            (id, schedule_id, tick_ts, status, claimed_by, claimed_at)
        VALUES ($1, $2, $3, 'claimed', $4, now())
        ON CONFLICT (schedule_id, tick_ts) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(tick_id)
    .bind(schedule_id)
    .bind(tick_ts)
    .bind(claimed_by)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(r) => ClaimOutcome::Claimed {
            tick_id: r.try_get("id")?,
        },
        None => ClaimOutcome::AlreadyClaimed,
    })
}

/// Mark abandoned `claimed` ticks (no `completed_at`, `claimed_at` older than
/// `REAPER_IDLE_SECONDS`) as `rolled_forward`. Returns the rolled-forward
/// rows enriched with their owning `project_id` and `schedule_name` so the
/// caller can emit a fully-populated `schedule.tick_rolled_forward` event —
/// the SSE route filters by `project_id`, so an event with `Uuid::nil()`
/// would be silently dropped at fanout.
///
/// Implemented as a CTE so the JOIN runs against the post-update tuples in
/// one round trip.
pub async fn reap_orphans(pool: &PgPool) -> Result<Vec<ReapedTick>, WorkerError> {
    let cutoff = Utc::now() - Duration::seconds(REAPER_IDLE_SECONDS);
    let rows = sqlx::query(
        r#"
        WITH reaped AS (
            UPDATE schedule_ticks
            SET status = 'rolled_forward'
            WHERE status = 'claimed'
              AND completed_at IS NULL
              AND claimed_at < $1
            RETURNING id, schedule_id, tick_ts
        )
        SELECT
            reaped.id           AS tick_id,
            reaped.schedule_id  AS schedule_id,
            reaped.tick_ts      AS tick_ts,
            schedules.project_id AS project_id,
            schedules.name      AS schedule_name
        FROM reaped
        JOIN schedules ON schedules.id = reaped.schedule_id
        "#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(ReapedTick {
                tick_id: r.try_get("tick_id")?,
                schedule_id: r.try_get("schedule_id")?,
                tick_ts: r.try_get("tick_ts")?,
                project_id: r.try_get("project_id")?,
                schedule_name: r.try_get("schedule_name")?,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReapedTick {
    pub tick_id: Uuid,
    pub schedule_id: Uuid,
    pub tick_ts: DateTime<Utc>,
    pub project_id: Uuid,
    pub schedule_name: String,
}

/// Compute the next anchored tick strictly after `last_run`. Anchored means
/// aligned to a fixed epoch boundary: the next tick is the smallest
/// `t = k * interval` (in minutes-since-unix-epoch) such that `t > last_run`.
///
/// Concrete consequences:
/// - `daily` (interval 1440 min): next UTC midnight after `last_run`.
/// - `hourly` (interval 60 min): next hour boundary.
/// - `every 6 hours` (interval 360 min): next 6-hour slot at 00:00, 06:00,
///   12:00, 18:00 UTC (these align with epoch since 1440 % 360 == 0).
/// - `every 15 minutes` (interval 15 min): next 15-min boundary.
/// - `every 48 hours` (interval 2880 min) / `weekly` (10080 min): next slot at
///   an epoch-aligned multi-day boundary. Critically, the result is strictly
///   after `last_run`, even when the cadence exceeds 24 hours. (The earlier
///   day-of-month-only formulation broke for intervals > 1440 min.)
///
/// The "next aligned slot" rule is what makes a worker that comes back online
/// after a 2.5-hour outage record `missed` for the skipped slot and fire on
/// the next aligned slot rather than firing immediately.
pub fn anchor_next_tick(cadence: Cadence, last_run: DateTime<Utc>) -> DateTime<Utc> {
    let interval_minutes = (1440.0 / cadence.ticks_per_day).round() as i64;
    if interval_minutes <= 0 {
        return last_run + Duration::minutes(1);
    }
    let epoch_minutes = last_run.timestamp().div_euclid(60);
    let next_slot_epoch_minutes =
        epoch_minutes.div_euclid(interval_minutes).saturating_add(1) * interval_minutes;
    DateTime::<Utc>::from_timestamp(next_slot_epoch_minutes * 60, 0)
        .unwrap_or(last_run + Duration::minutes(interval_minutes))
}

/// Convenience: parse the cadence string and return the next anchored tick.
pub fn next_tick_for(
    cadence_expr: &str,
    last_run: DateTime<Utc>,
) -> Result<DateTime<Utc>, ScheduleValidationError> {
    match parse_recurrence(cadence_expr)? {
        Recurrence::Frequency(cadence) => Ok(anchor_next_tick(cadence, last_run)),
        Recurrence::Calendar(spec) => next_calendar_tick(&spec, last_run)
            .ok_or_else(|| ScheduleValidationError::UnsupportedCadence(cadence_expr.into())),
    }
}

/// Compute the next wall-clock occurrence of a calendar recurrence strictly
/// after `last_run`. Walks forward day-by-day in the recurrence timezone,
/// landing on the first matching day at `hour:minute`. Returns `None` when the
/// timezone is unknown or no occurrence is found within a bounded horizon
/// (defensive — every well-formed spec resolves within a handful of days).
///
/// DST handling: a local time that doesn't exist (spring-forward gap) or is
/// ambiguous (fall-back) is resolved to the earliest valid instant; a fully
/// nonexistent local time on a given day causes that day to be skipped.
fn next_calendar_tick(spec: &CalendarSpec, last_run: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let tz: Tz = spec.tz.parse().ok()?;
    let local_anchor = last_run.with_timezone(&tz);
    let anchor_date = local_anchor.date_naive();

    // Bounded horizon: daily/weekly resolve within 7 days; every-N-days within
    // 2N. 800 days is a generous ceiling that also guards malformed input.
    for offset in 0..800 {
        let date = anchor_date.checked_add_signed(Duration::days(offset))?;
        if !day_matches(&spec.cadence, anchor_date, date) {
            continue;
        }
        let naive = date.and_hms_opt(spec.hour, spec.minute, 0)?;
        let Some(local) = tz.from_local_datetime(&naive).earliest() else {
            continue; // local time falls in a DST gap on this date.
        };
        let instant = local.with_timezone(&Utc);
        if instant > last_run {
            return Some(instant);
        }
    }
    None
}

/// Whether `date` satisfies the cadence's day rule, given `anchor` (the local
/// date of the last run) for every-N-days phase alignment.
fn day_matches(cadence: &CalendarCadence, anchor: NaiveDate, date: NaiveDate) -> bool {
    match cadence {
        CalendarCadence::Daily => true,
        CalendarCadence::Weekly(days) => {
            let dow = date.weekday().num_days_from_sunday();
            days.contains(&dow)
        }
        CalendarCadence::EveryNDays(n) => {
            let diff = (date - anchor).num_days();
            diff >= 0 && diff % (*n as i64) == 0
        }
    }
}

/// Returns true when `now` is within `debounce_minutes` of `last_manual_run`.
/// Used by FR-25 debounce: a manual `ogeo prompt run` within 5 min of a
/// scheduled tick suppresses the tick (`debounced` status, zero Provider
/// calls).
pub fn is_debounced(
    last_manual_run: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    debounce_minutes: i64,
) -> bool {
    let Some(last) = last_manual_run else {
        return false;
    };
    now.signed_duration_since(last) < Duration::minutes(debounce_minutes)
}

/// Construct a broadcast channel for lifecycle events. The API's SSE handler,
/// webhook dispatcher, and notification channels each subscribe; the worker
/// owns the sender.
pub fn event_channel() -> (
    broadcast::Sender<LifecycleEvent>,
    broadcast::Receiver<LifecycleEvent>,
) {
    broadcast::channel(EVENT_CHANNEL_CAPACITY)
}

/// Best-effort emit. A full channel drops the event for the slowest
/// subscriber; the worker never blocks waiting for delivery (NFR — worker
/// progress must not depend on subscriber backpressure).
pub fn emit(sender: &broadcast::Sender<LifecycleEvent>, event: LifecycleEvent) {
    let _ = sender.send(event);
}

/// Construct a `SchedulePayload` for one event emission. `event_id` is fresh
/// per call: each emission (`tick_claimed`, `tick_completed`, etc.) gets its
/// own event_id; subscribers (SSE, webhook deliveries, notification channels)
/// share the same event_id within one emission's fanout, not across the
/// tick's whole lifecycle.
pub fn payload_for(
    project_id: Uuid,
    schedule_id: Uuid,
    schedule_name: &str,
    tick_id: Uuid,
    tick_ts: DateTime<Utc>,
) -> SchedulePayload {
    SchedulePayload {
        event_id: Uuid::from_u128(ulid::Ulid::new().0),
        project_id,
        schedule_id,
        schedule_name: schedule_name.to_owned(),
        tick_id,
        tick_ts,
        emitted_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_cadence;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 28, h, m, 0).unwrap()
    }

    #[test]
    fn anchors_hourly_to_next_hour_boundary() {
        let cadence = parse_cadence("hourly").unwrap();
        let next = anchor_next_tick(cadence, at(8, 23));
        assert_eq!(next, at(9, 0));
    }

    #[test]
    fn anchors_daily_to_next_utc_midnight() {
        let cadence = parse_cadence("daily").unwrap();
        let next = anchor_next_tick(cadence, at(14, 23));
        // Daily anchors to midnight of the next day, computed as the next
        // 1440-minute slot after `last_run`.
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 29, 0, 0, 0).unwrap());
    }

    #[test]
    fn anchors_every_6_hours_to_next_6_hour_slot() {
        let cadence = parse_cadence("every 6 hours").unwrap();
        let next = anchor_next_tick(cadence, at(8, 0));
        assert_eq!(next, at(12, 0));
    }

    #[test]
    fn anchors_every_15_minutes_to_next_quarter_hour() {
        let cadence = parse_cadence("every 15 minutes").unwrap();
        let next = anchor_next_tick(cadence, at(8, 7));
        assert_eq!(next, at(8, 15));
    }

    #[test]
    fn debounce_suppresses_within_window() {
        let now = at(8, 4);
        let last = Some(at(8, 0));
        assert!(is_debounced(last, now, 5));
    }

    #[test]
    fn debounce_does_not_suppress_after_window() {
        let now = at(8, 6);
        let last = Some(at(8, 0));
        assert!(!is_debounced(last, now, 5));
    }

    #[test]
    fn debounce_does_not_suppress_when_no_prior_manual_run() {
        assert!(!is_debounced(None, at(8, 0), 5));
    }

    #[test]
    fn emit_does_not_block_when_no_subscribers() {
        let (tx, _rx) = event_channel();
        // Drop the only receiver, then emit. Must not panic / block.
        // (broadcast::send returns Err when there are zero receivers; emit
        // discards the result.)
        drop(_rx);
        emit(
            &tx,
            LifecycleEvent::Missed(SchedulePayload {
                event_id: Uuid::nil(),
                project_id: Uuid::nil(),
                schedule_id: Uuid::nil(),
                schedule_name: "x".into(),
                tick_id: Uuid::nil(),
                tick_ts: at(8, 0),
                emitted_at: at(8, 0),
            }),
        );
    }

    #[test]
    fn next_tick_for_parses_and_anchors() {
        let next = next_tick_for("hourly", at(8, 30)).unwrap();
        assert_eq!(next, at(9, 0));
    }

    #[test]
    fn anchors_weekly_to_strictly_future_slot() {
        // Pre-fix bug: cadences with interval > 1440 min anchored to the
        // start of last_run's day, producing a result < 24 h away.
        // Post-fix: the result must be strictly after last_run by at least
        // the cadence interval boundary.
        let cadence = parse_cadence("weekly").unwrap();
        let next = anchor_next_tick(cadence, at(14, 23));
        let interval = Duration::minutes(10080);
        assert!(
            (next - at(14, 23)) > Duration::zero(),
            "next tick must be strictly future, got {next}"
        );
        assert!(
            (next - at(14, 23)) <= interval,
            "next tick must land within one cadence interval, got {next}"
        );
    }

    #[test]
    fn anchors_every_2_days_to_epoch_aligned_slot() {
        // Verify the epoch-anchored shape for intervals > 24 h.
        // Every-48-h slots align to 1970-01-01T00:00 + 2N days, so the
        // next slot after any timestamp is the next "even unix-day" midnight.
        let cadence = super::Cadence { ticks_per_day: 0.5 }; // every 48 h
        let next = anchor_next_tick(cadence, at(14, 23));
        // Next slot is strictly after, lands on a midnight, and within 48 h.
        assert!(next > at(14, 23));
        assert_eq!(next.timestamp() % 60, 0);
        assert_eq!(next.timestamp() % 3600, 0); // hour-aligned
        assert!((next - at(14, 23)) <= Duration::hours(48));
    }

    #[test]
    fn calendar_daily_lands_on_local_time_of_day() {
        // 2026-05-28 is EDT (UTC-4). 09:30 local == 13:30 UTC.
        let next = next_tick_for("TZ=America/New_York daily at 09:30", at(8, 0)).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 28, 13, 30, 0).unwrap());
    }

    #[test]
    fn calendar_daily_rolls_to_next_day_when_time_passed() {
        // last_run at 18:00 UTC (14:00 EDT) is past today's 09:30 local slot,
        // so the next occurrence is tomorrow at 09:30 EDT == 13:30 UTC.
        let next = next_tick_for("TZ=America/New_York daily at 09:30", at(18, 0)).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 29, 13, 30, 0).unwrap());
    }

    #[test]
    fn calendar_weekly_picks_next_listed_weekday() {
        // 2026-05-28 is a Thursday. Next Mon/Fri after Thu 08:00 UTC is
        // Friday 2026-05-29 at 09:00 EDT == 13:00 UTC.
        let next =
            next_tick_for("TZ=America/New_York weekly on mon,fri at 09:00", at(8, 0)).unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 29, 13, 0, 0).unwrap());
    }

    #[test]
    fn calendar_every_n_days_anchors_to_last_run() {
        // Anchor date 2026-05-28; every 3 days → next slot at +3 days at 06:00
        // local, since today's 06:00 local already passed (last_run 08:00 UTC
        // == 04:00 EDT, before 06:00, so actually fires today).
        let next = next_tick_for("TZ=America/New_York every 3 days at 06:00", at(8, 0)).unwrap();
        // 06:00 EDT == 10:00 UTC, still after 08:00 UTC → today.
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 5, 28, 10, 0, 0).unwrap());
    }

    #[test]
    fn calendar_unknown_timezone_is_unsupported() {
        assert!(next_tick_for("TZ=Mars/Olympus daily at 09:30", at(8, 0)).is_err());
    }
}
