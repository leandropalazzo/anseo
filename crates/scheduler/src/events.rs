//! Phase 2 lifecycle event payloads (ARCH-17). The worker emits these onto a
//! `tokio::sync::broadcast` channel; the API's SSE endpoint forwards them to
//! subscribers, the webhook dispatcher persists them as deliveries, and the
//! notification channels project them into Slack/SMTP.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wire-stable string identifiers for ARCH-17 event kinds. Used as the SSE
/// `event:` field and as the persisted `webhook_deliveries.event_kind`.
pub mod kinds {
    pub const SCHEDULE_TICK_PLANNED: &str = "schedule.tick_planned";
    pub const SCHEDULE_TICK_CLAIMED: &str = "schedule.tick_claimed";
    pub const SCHEDULE_TICK_COMPLETED: &str = "schedule.tick_completed";
    pub const SCHEDULE_TICK_FAILED: &str = "schedule.tick_failed";
    pub const SCHEDULE_TICK_CAPPED: &str = "schedule.tick_capped";
    pub const SCHEDULE_TICK_ROLLED_FORWARD: &str = "schedule.tick_rolled_forward";
    pub const SCHEDULE_MISSED: &str = "schedule.missed";
    pub const SCHEDULE_DEBOUNCED: &str = "schedule.debounced";
    pub const VISIBILITY_ANOMALY: &str = "visibility.anomaly";
    pub const CITATION_ANOMALY: &str = "citation.anomaly";
    // Story 19.6 — GEO Recommendation lifecycle events. Reuse the Phase 2
    // HMAC signer + retry ladder unchanged (architecture §4.4 webhook surface).
    pub const RECOMMENDATION_GENERATED: &str = "recommendation.generated";
    pub const RECOMMENDATION_SURFACED: &str = "recommendation.surfaced";
    pub const RECOMMENDATION_ACTED: &str = "recommendation.acted";
    pub const RECOMMENDATION_MEASURED: &str = "recommendation.measured";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchedulePayload {
    pub event_id: Uuid,
    pub project_id: Uuid,
    pub schedule_id: Uuid,
    pub schedule_name: String,
    pub tick_id: Uuid,
    pub tick_ts: DateTime<Utc>,
    pub emitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletedPayload {
    #[serde(flatten)]
    pub base: SchedulePayload,
    pub prompt_run_count: u32,
    pub failed_run_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FailedPayload {
    #[serde(flatten)]
    pub base: SchedulePayload,
    pub error_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CappedPayload {
    #[serde(flatten)]
    pub base: SchedulePayload,
    pub cap_name: String,
    pub cap_threshold: f64,
    pub projected_value: f64,
}

/// Anomaly-detector verdict carried over the SSE / webhook / notification
/// fanout. Story 10.3 produces these from the analytics crate's
/// `anomaly::AnomalyVerdict`; the worker translates each verdict into one
/// `LifecycleEvent::VisibilityAnomaly` or `CitationAnomaly` emission via
/// `AnomalyVerdict.provider.as_wire_str()` (the wire shape is a string, not
/// the typed `ProviderName` — kept stable for non-Rust consumers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnomalyPayload {
    pub event_id: Uuid,
    pub project_id: Uuid,
    /// Wire-stable provider name; see `ProviderName::as_wire_str`.
    pub provider: String,
    pub observed_at: DateTime<Utc>,
    pub summary: String,
    pub detail: serde_json::Value,
    pub emitted_at: DateTime<Utc>,
}

/// GEO Recommendation lifecycle payload (Story 19.6). Carries the
/// recommendation identity + the state it transitioned into, so a webhook
/// consumer can route on `state` without a follow-up GET. The wire shape uses
/// plain strings for `kind`/`state` (not the typed Rust enums) to stay stable
/// for non-Rust consumers, matching the [`AnomalyPayload`] convention.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecommendationPayload {
    pub event_id: Uuid,
    pub project_id: Uuid,
    pub recommendation_id: Uuid,
    /// The Recommendation Kind wire string (e.g. `docs_not_cited_for_prompt`).
    pub recommendation_kind: String,
    /// The lifecycle state this event marks (`generated`/`surfaced`/`acted`/
    /// `measured`).
    pub state: String,
    pub summary: String,
    pub emitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum LifecycleEvent {
    #[serde(rename = "schedule.tick_planned")]
    TickPlanned(SchedulePayload),
    #[serde(rename = "schedule.tick_claimed")]
    TickClaimed(SchedulePayload),
    #[serde(rename = "schedule.tick_completed")]
    TickCompleted(CompletedPayload),
    #[serde(rename = "schedule.tick_failed")]
    TickFailed(FailedPayload),
    #[serde(rename = "schedule.tick_capped")]
    TickCapped(CappedPayload),
    #[serde(rename = "schedule.tick_rolled_forward")]
    TickRolledForward(SchedulePayload),
    #[serde(rename = "schedule.missed")]
    Missed(SchedulePayload),
    #[serde(rename = "schedule.debounced")]
    Debounced(SchedulePayload),
    #[serde(rename = "visibility.anomaly")]
    VisibilityAnomaly(AnomalyPayload),
    #[serde(rename = "citation.anomaly")]
    CitationAnomaly(AnomalyPayload),
    #[serde(rename = "recommendation.generated")]
    RecommendationGenerated(RecommendationPayload),
    #[serde(rename = "recommendation.surfaced")]
    RecommendationSurfaced(RecommendationPayload),
    #[serde(rename = "recommendation.acted")]
    RecommendationActed(RecommendationPayload),
    #[serde(rename = "recommendation.measured")]
    RecommendationMeasured(RecommendationPayload),
}

impl LifecycleEvent {
    /// Wire-stable kind string for SSE `event:` and persisted delivery rows.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TickPlanned(_) => kinds::SCHEDULE_TICK_PLANNED,
            Self::TickClaimed(_) => kinds::SCHEDULE_TICK_CLAIMED,
            Self::TickCompleted(_) => kinds::SCHEDULE_TICK_COMPLETED,
            Self::TickFailed(_) => kinds::SCHEDULE_TICK_FAILED,
            Self::TickCapped(_) => kinds::SCHEDULE_TICK_CAPPED,
            Self::TickRolledForward(_) => kinds::SCHEDULE_TICK_ROLLED_FORWARD,
            Self::Missed(_) => kinds::SCHEDULE_MISSED,
            Self::Debounced(_) => kinds::SCHEDULE_DEBOUNCED,
            Self::VisibilityAnomaly(_) => kinds::VISIBILITY_ANOMALY,
            Self::CitationAnomaly(_) => kinds::CITATION_ANOMALY,
            Self::RecommendationGenerated(_) => kinds::RECOMMENDATION_GENERATED,
            Self::RecommendationSurfaced(_) => kinds::RECOMMENDATION_SURFACED,
            Self::RecommendationActed(_) => kinds::RECOMMENDATION_ACTED,
            Self::RecommendationMeasured(_) => kinds::RECOMMENDATION_MEASURED,
        }
    }

    pub fn project_id(&self) -> Uuid {
        match self {
            Self::TickPlanned(p)
            | Self::TickClaimed(p)
            | Self::TickRolledForward(p)
            | Self::Missed(p)
            | Self::Debounced(p) => p.project_id,
            Self::TickCompleted(p) => p.base.project_id,
            Self::TickFailed(p) => p.base.project_id,
            Self::TickCapped(p) => p.base.project_id,
            Self::VisibilityAnomaly(p) | Self::CitationAnomaly(p) => p.project_id,
            Self::RecommendationGenerated(p)
            | Self::RecommendationSurfaced(p)
            | Self::RecommendationActed(p)
            | Self::RecommendationMeasured(p) => p.project_id,
        }
    }

    pub fn event_id(&self) -> Uuid {
        match self {
            Self::TickPlanned(p)
            | Self::TickClaimed(p)
            | Self::TickRolledForward(p)
            | Self::Missed(p)
            | Self::Debounced(p) => p.event_id,
            Self::TickCompleted(p) => p.base.event_id,
            Self::TickFailed(p) => p.base.event_id,
            Self::TickCapped(p) => p.base.event_id,
            Self::VisibilityAnomaly(p) | Self::CitationAnomaly(p) => p.event_id,
            Self::RecommendationGenerated(p)
            | Self::RecommendationSurfaced(p)
            | Self::RecommendationActed(p)
            | Self::RecommendationMeasured(p) => p.event_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_payload() -> SchedulePayload {
        SchedulePayload {
            event_id: Uuid::nil(),
            project_id: Uuid::nil(),
            schedule_id: Uuid::nil(),
            schedule_name: "daily-check".into(),
            tick_id: Uuid::nil(),
            tick_ts: Utc.with_ymd_and_hms(2026, 5, 28, 12, 0, 0).unwrap(),
            emitted_at: Utc.with_ymd_and_hms(2026, 5, 28, 12, 0, 1).unwrap(),
        }
    }

    #[test]
    fn kind_string_matches_arch_17_wire_form() {
        let evt = LifecycleEvent::TickPlanned(sample_payload());
        assert_eq!(evt.kind(), "schedule.tick_planned");
    }

    #[test]
    fn serialized_payload_uses_kind_discriminator() {
        let evt = LifecycleEvent::TickPlanned(sample_payload());
        let json = serde_json::to_value(&evt).unwrap();
        assert_eq!(json["kind"], "schedule.tick_planned");
        assert_eq!(json["schedule_name"], "daily-check");
    }

    #[test]
    fn completed_payload_round_trips_run_counts() {
        let evt = LifecycleEvent::TickCompleted(CompletedPayload {
            base: sample_payload(),
            prompt_run_count: 3,
            failed_run_count: 1,
        });
        let json = serde_json::to_string(&evt).unwrap();
        let back: LifecycleEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, evt);
    }

    #[test]
    fn capped_payload_surfaces_cap_name_and_threshold() {
        let evt = LifecycleEvent::TickCapped(CappedPayload {
            base: sample_payload(),
            cap_name: "project_daily_usd".into(),
            cap_threshold: 50.0,
            projected_value: 71.25,
        });
        let json = serde_json::to_value(&evt).unwrap();
        assert_eq!(json["cap_name"], "project_daily_usd");
        assert_eq!(json["cap_threshold"], 50.0);
    }

    fn sample_rec_payload(state: &str) -> RecommendationPayload {
        RecommendationPayload {
            event_id: Uuid::nil(),
            project_id: Uuid::nil(),
            recommendation_id: Uuid::nil(),
            recommendation_kind: "docs_not_cited_for_prompt".into(),
            state: state.into(),
            summary: "docs not cited".into(),
            emitted_at: Utc.with_ymd_and_hms(2026, 5, 30, 12, 0, 0).unwrap(),
        }
    }

    #[test]
    fn recommendation_events_carry_arch_44_kind_strings() {
        let cases = [
            (
                LifecycleEvent::RecommendationGenerated(sample_rec_payload("generated")),
                "recommendation.generated",
            ),
            (
                LifecycleEvent::RecommendationSurfaced(sample_rec_payload("surfaced")),
                "recommendation.surfaced",
            ),
            (
                LifecycleEvent::RecommendationActed(sample_rec_payload("acted")),
                "recommendation.acted",
            ),
            (
                LifecycleEvent::RecommendationMeasured(sample_rec_payload("measured")),
                "recommendation.measured",
            ),
        ];
        for (evt, want) in cases {
            assert_eq!(evt.kind(), want);
            let json = serde_json::to_value(&evt).unwrap();
            assert_eq!(json["kind"], want);
            // Round-trips through the tagged enum verbatim.
            let back: LifecycleEvent = serde_json::from_value(json).unwrap();
            assert_eq!(back, evt);
        }
    }

    #[test]
    fn every_kind_variant_emits_a_distinct_wire_string() {
        let payload = sample_payload();
        let kinds = [
            LifecycleEvent::TickPlanned(payload.clone()).kind(),
            LifecycleEvent::TickClaimed(payload.clone()).kind(),
            LifecycleEvent::TickCompleted(CompletedPayload {
                base: payload.clone(),
                prompt_run_count: 0,
                failed_run_count: 0,
            })
            .kind(),
            LifecycleEvent::TickFailed(FailedPayload {
                base: payload.clone(),
                error_message: "x".into(),
            })
            .kind(),
            LifecycleEvent::TickCapped(CappedPayload {
                base: payload.clone(),
                cap_name: "x".into(),
                cap_threshold: 0.0,
                projected_value: 0.0,
            })
            .kind(),
            LifecycleEvent::TickRolledForward(payload.clone()).kind(),
            LifecycleEvent::Missed(payload.clone()).kind(),
            LifecycleEvent::Debounced(payload).kind(),
        ];
        let mut sorted: Vec<&str> = kinds.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), kinds.len(), "kind strings must be distinct");
    }
}
