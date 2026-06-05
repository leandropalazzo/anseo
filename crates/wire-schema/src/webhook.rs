//! Phase 2 webhook event payload schemas (architecture §5.3, C-12, FR-35).
//!
//! Each event is wire-stable JSON with a shared envelope and a variant-
//! specific `data` block. The envelope discriminator is `event_kind`
//! (matches `LifecycleEvent::kind()` in `crates/scheduler/src/events.rs`
//! for the variants that overlap with the SSE stream).
//!
//! Wire shape:
//!
//! ```jsonc
//! {
//!   "event_kind": "prompt_run.completed",
//!   "event_id": "01HXYZ...",          // ULID; idempotency key for retries
//!   "occurred_at": "2026-06-15T08:00:43.221Z",
//!   "project_id": "01H...",
//!   "data": { /* variant-specific */ }
//! }
//! ```
//!
//! This module is the source-of-truth for the bytes a webhook consumer
//! receives. Architecture §5.2 wraps these payloads in the
//! `X-Anseo-Signature: v1=t=…,s=…` HMAC envelope; verification logic
//! lives in `anseo_scheduler::webhooks::signer`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// The five Phase 2 webhook event kinds (architecture §5.3).
/// Wire-stable strings; downstream consumers (SDK clients, Slack/SMTP
/// channels, the Dashboard) pattern-match on these.
pub mod kinds {
    pub const PROMPT_RUN_COMPLETED: &str = "prompt_run.completed";
    pub const VISIBILITY_REGRESSION: &str = "visibility.regression";
    pub const SCHEDULE_MISSED: &str = "schedule.missed";
    pub const VISIBILITY_ANOMALY: &str = "visibility.anomaly";
    pub const CITATION_ANOMALY: &str = "citation.anomaly";
}

/// `prompt_run.completed` data block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PromptRunCompletedData {
    pub run_id: Ulid,
    pub prompt_id: Ulid,
    pub provider: String,
    pub model: String,
    pub status: String,
    /// 1 = top, higher = lower visibility, `None` = not present.
    pub ranking: Option<i32>,
    pub mention_count: u32,
    pub duration_ms: u64,
    pub error_kind: Option<String>,
    /// `manual` | `api` | `scheduled:<schedule_id>` (architecture §5.3).
    pub triggered_by: String,
}

/// `visibility.regression` data block. Emitted by the analytics path when
/// a prompt's recent average ranking degrades against the prior window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VisibilityRegressionData {
    pub prompt_id: Ulid,
    pub provider: String,
    pub previous_avg_rank: f64,
    pub current_avg_rank: f64,
    pub window_days: u32,
}

/// `schedule.missed` data block. The worker emits this when a scheduled
/// tick passed without execution (worker offline, or capped by Provider
/// density rules).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleMissedData {
    pub schedule_id: Ulid,
    pub schedule_name: String,
    pub tick_ts: DateTime<Utc>,
    pub reason: String,
}

/// Shared envelope for both `visibility.anomaly` and `citation.anomaly`.
/// Matches `anseo_scheduler::events::AnomalyPayload` byte-for-byte so
/// the SSE → webhook fanout is one serde call, not two shapes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnomalyData {
    pub provider: String,
    pub observed_at: DateTime<Utc>,
    pub summary: String,
    pub detail: serde_json::Value,
}

/// Wire envelope with the variant `data` parametrized. Use the
/// concrete aliases below from handler code.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebhookEvent<T> {
    pub event_kind: String,
    pub event_id: Ulid,
    pub occurred_at: DateTime<Utc>,
    pub project_id: Ulid,
    pub data: T,
}

pub type PromptRunCompletedEvent = WebhookEvent<PromptRunCompletedData>;
pub type VisibilityRegressionEvent = WebhookEvent<VisibilityRegressionData>;
pub type ScheduleMissedEvent = WebhookEvent<ScheduleMissedData>;
pub type VisibilityAnomalyEvent = WebhookEvent<AnomalyData>;
pub type CitationAnomalyEvent = WebhookEvent<AnomalyData>;

impl<T> WebhookEvent<T> {
    /// Construct an event with a fresh `event_id` and `occurred_at = now`.
    /// Production callers use this so retried deliveries reuse the same
    /// `event_id` (idempotency); a fresh one only fires on the first
    /// emission.
    pub fn new(event_kind: impl Into<String>, project_id: Ulid, data: T) -> Self {
        Self {
            event_kind: event_kind.into(),
            event_id: Ulid::new(),
            occurred_at: Utc::now(),
            project_id,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_project() -> Ulid {
        Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    fn fixed_ulid_a() -> Ulid {
        Ulid::from_string("01HXYZ000000000000000000AA").unwrap()
    }

    fn fixed_ulid_b() -> Ulid {
        Ulid::from_string("01HXYZ000000000000000000BB").unwrap()
    }

    #[test]
    fn kind_constants_match_arch_5_3() {
        assert_eq!(kinds::PROMPT_RUN_COMPLETED, "prompt_run.completed");
        assert_eq!(kinds::VISIBILITY_REGRESSION, "visibility.regression");
        assert_eq!(kinds::SCHEDULE_MISSED, "schedule.missed");
        assert_eq!(kinds::VISIBILITY_ANOMALY, "visibility.anomaly");
        assert_eq!(kinds::CITATION_ANOMALY, "citation.anomaly");
    }

    #[test]
    fn prompt_run_completed_serializes_canonical_shape() {
        let event = PromptRunCompletedEvent {
            event_kind: kinds::PROMPT_RUN_COMPLETED.into(),
            event_id: fixed_ulid_a(),
            occurred_at: Utc.with_ymd_and_hms(2026, 6, 15, 8, 0, 43).unwrap(),
            project_id: fixed_project(),
            data: PromptRunCompletedData {
                run_id: fixed_ulid_b(),
                prompt_id: fixed_ulid_a(),
                provider: "openai".into(),
                model: "gpt-4o-2024-08-06".into(),
                status: "ok".into(),
                ranking: Some(2),
                mention_count: 3,
                duration_ms: 4221,
                error_kind: None,
                triggered_by: "scheduled:01H000".into(),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event_kind"], "prompt_run.completed");
        assert_eq!(json["data"]["provider"], "openai");
        assert_eq!(json["data"]["ranking"], 2);
        assert_eq!(json["data"]["error_kind"], serde_json::Value::Null);

        // Round-trip through JSON to pin the wire-deserialize.
        let bytes = serde_json::to_vec(&event).unwrap();
        let back: PromptRunCompletedEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn anomaly_event_round_trips_detail_blob() {
        let event = VisibilityAnomalyEvent {
            event_kind: kinds::VISIBILITY_ANOMALY.into(),
            event_id: fixed_ulid_a(),
            occurred_at: Utc::now(),
            project_id: fixed_project(),
            data: AnomalyData {
                provider: "anthropic".into(),
                observed_at: Utc::now(),
                summary: "z=3.4, rank=8.0, prev=2.1±0.3".into(),
                detail: serde_json::json!({
                    "signal": "zscore",
                    "z": 3.4,
                    "window_mean": 2.1,
                    "window_stddev": 0.3,
                }),
            },
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        let back: VisibilityAnomalyEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, event);
        assert_eq!(back.data.detail["signal"], "zscore");
    }

    #[test]
    fn schedule_missed_carries_reason_string() {
        let event = ScheduleMissedEvent {
            event_kind: kinds::SCHEDULE_MISSED.into(),
            event_id: fixed_ulid_a(),
            occurred_at: Utc::now(),
            project_id: fixed_project(),
            data: ScheduleMissedData {
                schedule_id: fixed_ulid_b(),
                schedule_name: "daily-check".into(),
                tick_ts: Utc.with_ymd_and_hms(2026, 6, 15, 8, 0, 0).unwrap(),
                reason: "worker_offline".into(),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["data"]["reason"], "worker_offline");
        assert_eq!(json["data"]["schedule_name"], "daily-check");
    }

    #[test]
    fn new_constructor_generates_ulid_and_uses_now() {
        let evt = WebhookEvent::new(
            kinds::SCHEDULE_MISSED,
            fixed_project(),
            ScheduleMissedData {
                schedule_id: fixed_ulid_a(),
                schedule_name: "x".into(),
                tick_ts: Utc::now(),
                reason: "test".into(),
            },
        );
        assert_eq!(evt.event_kind, "schedule.missed");
        // event_id should be a freshly-generated ULID, not the nil one.
        assert_ne!(evt.event_id, Ulid::nil());
    }

    #[test]
    fn visibility_regression_persists_window_days() {
        let event = VisibilityRegressionEvent {
            event_kind: kinds::VISIBILITY_REGRESSION.into(),
            event_id: fixed_ulid_a(),
            occurred_at: Utc::now(),
            project_id: fixed_project(),
            data: VisibilityRegressionData {
                prompt_id: fixed_ulid_b(),
                provider: "openai".into(),
                previous_avg_rank: 2.0,
                current_avg_rank: 5.4,
                window_days: 14,
            },
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        let back: VisibilityRegressionEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.data.window_days, 14);
    }
}
