//! Story 19.4 — Recommendation lifecycle state machine + outcome measurement
//! (architecture §6). Pure over its inputs: the API endpoint (Story 19.6)
//! persists transitions to `recommendation_lifecycle_events` and maps
//! [`LifecycleError::IllegalTransition`] to HTTP 409 (`[rec-3]`).
//!
//! `State` here is the DB-aligned lifecycle (§7.1 CHECK) and is intentionally
//! separate from the wire `LifecycleState` (which represents only the
//! freshly-generated envelope) so the 19.1/19.2 wire contract is untouched.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Generated,
    Surfaced,
    Acknowledged,
    Acted,
    Measured,
    Dismissed,
    Stale,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Generated => "generated",
            State::Surfaced => "surfaced",
            State::Acknowledged => "acknowledged",
            State::Acted => "acted",
            State::Measured => "measured",
            State::Dismissed => "dismissed",
            State::Stale => "stale",
        }
    }

    /// Terminal states accept no further transitions (§6.1).
    pub fn is_terminal(&self) -> bool {
        matches!(self, State::Measured | State::Dismissed | State::Stale)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleError {
    #[error("illegal transition: {from} -> {to}")]
    IllegalTransition {
        from: &'static str,
        to: &'static str,
    },
}

/// Whether `from -> to` is a legal lifecycle edge (§6.1). A self-transition is
/// legal and idempotent (replaying is a no-op).
pub fn can_transition(from: State, to: State) -> bool {
    use State::*;
    if from == to {
        return true; // idempotent replay
    }
    matches!(
        (from, to),
        (Generated, Surfaced)
            | (Generated, Stale)
            | (Surfaced, Acknowledged)
            | (Surfaced, Dismissed)
            | (Acknowledged, Acted)
            | (Acknowledged, Dismissed)
            | (Acted, Measured)
    )
}

/// Apply a transition. Idempotent: `from == to` returns `Ok(to)` unchanged.
pub fn transition(from: State, to: State) -> Result<State, LifecycleError> {
    if can_transition(from, to) {
        Ok(to)
    } else {
        Err(LifecycleError::IllegalTransition {
            from: from.as_str(),
            to: to.as_str(),
        })
    }
}

/// A lifecycle warning surfaced alongside a transition (decision L4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleWarning {
    pub kind: String,
}

/// `recommend.mark_acted` (decision L4): transition `Acknowledged -> Acted`
/// with a free-form note + optional evidence_url. An absent evidence_url
/// emits a single `lifecycle.evidence_missing` warning (non-fatal).
#[derive(Debug, Clone)]
pub struct MarkActedResult {
    pub state: State,
    pub warnings: Vec<LifecycleWarning>,
}

pub fn mark_acted(
    from: State,
    _note: Option<&str>,
    evidence_url: Option<&str>,
) -> Result<MarkActedResult, LifecycleError> {
    let state = transition(from, State::Acted)?;
    let mut warnings = Vec::new();
    if evidence_url.is_none_or(|u| u.trim().is_empty()) {
        warnings.push(LifecycleWarning {
            kind: "lifecycle.evidence_missing".to_string(),
        });
    }
    Ok(MarkActedResult { state, warnings })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Improved,
    Unchanged,
    Regressed,
    /// Too few post-Acted Prompt Runs to conclude (< [`MIN_POST_ACTED_RUNS`]).
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeasurementOutcome {
    pub measured_at: DateTime<Utc>,
    pub metric_name: String,
    pub value_at_acted: f64,
    pub value_at_measure: f64,
    pub status: OutcomeStatus,
}

/// Phase 3 GA threshold: need ≥ 10 post-Acted Prompt Runs to conclude (§6.3).
pub const MIN_POST_ACTED_RUNS: u32 = 10;

/// Default outcome window: 14 days after the Acted transition (§6.2, OQ-P3-17).
pub const OUTCOME_WINDOW_DAYS: i64 = 14;

/// When the deferred outcome measurement is due, given the Acted timestamp.
pub fn outcome_due_at(acted_at: DateTime<Utc>) -> DateTime<Utc> {
    acted_at + Duration::days(OUTCOME_WINDOW_DAYS)
}

/// Compute the §6.3 outcome. `inconclusive` when the post-Acted window has too
/// few Prompt Runs to be confident, regardless of the value delta.
pub fn measure_outcome(
    measured_at: DateTime<Utc>,
    metric_name: impl Into<String>,
    value_at_acted: f64,
    value_at_measure: f64,
    post_acted_runs: u32,
) -> MeasurementOutcome {
    let status = if post_acted_runs < MIN_POST_ACTED_RUNS {
        OutcomeStatus::Inconclusive
    } else if value_at_measure > value_at_acted {
        OutcomeStatus::Improved
    } else if value_at_measure < value_at_acted {
        OutcomeStatus::Regressed
    } else {
        OutcomeStatus::Unchanged
    };
    MeasurementOutcome {
        measured_at,
        metric_name: metric_name.into(),
        value_at_acted,
        value_at_measure,
        status,
    }
}
