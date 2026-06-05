//! Story 19.4 `[rec-3]` — lifecycle state machine: legal vs illegal
//! transitions (illegal → IllegalTransition, which the endpoint maps to 409),
//! `mark_acted` evidence-missing warning (decision L4), and the §6.3 outcome
//! measurement including the inconclusive (< 10 runs) branch.

use anseo_recommendations::lifecycle::{
    can_transition, mark_acted, measure_outcome, outcome_due_at, transition, LifecycleError,
    OutcomeStatus, State, MIN_POST_ACTED_RUNS, OUTCOME_WINDOW_DAYS,
};
use chrono::{DateTime, Duration, Utc};

const ALL: [State; 7] = [
    State::Generated,
    State::Surfaced,
    State::Acknowledged,
    State::Acted,
    State::Measured,
    State::Dismissed,
    State::Stale,
];

fn is_legal_edge(from: State, to: State) -> bool {
    use State::*;
    from == to
        || matches!(
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

/// Enumerate the full 7×7 transition matrix; assert legal edges succeed and
/// every other edge returns IllegalTransition.
#[test]
fn rec_3_full_transition_matrix() {
    for from in ALL {
        for to in ALL {
            let expected_legal = is_legal_edge(from, to);
            assert_eq!(
                can_transition(from, to),
                expected_legal,
                "{} -> {} legality",
                from.as_str(),
                to.as_str()
            );
            match transition(from, to) {
                Ok(s) => {
                    assert!(
                        expected_legal,
                        "{} -> {} should be illegal",
                        from.as_str(),
                        to.as_str()
                    );
                    assert_eq!(s, to);
                }
                Err(LifecycleError::IllegalTransition { .. }) => {
                    assert!(
                        !expected_legal,
                        "{} -> {} should be legal",
                        from.as_str(),
                        to.as_str()
                    );
                }
            }
        }
    }
}

/// Terminal states reject every non-self transition.
#[test]
fn terminal_states_are_sinks() {
    for term in [State::Measured, State::Dismissed, State::Stale] {
        assert!(term.is_terminal());
        for to in ALL.iter().filter(|s| **s != term) {
            assert!(
                transition(term, *to).is_err(),
                "{} must be terminal",
                term.as_str()
            );
        }
    }
}

/// Self-transitions are idempotent no-ops (replay safety).
#[test]
fn self_transitions_are_idempotent() {
    for s in ALL {
        assert_eq!(transition(s, s).unwrap(), s);
    }
}

/// mark_acted from Acknowledged succeeds; absent evidence_url emits exactly one
/// `lifecycle.evidence_missing` warning, present evidence_url emits none.
#[test]
fn mark_acted_evidence_missing_warning() {
    let no_ev = mark_acted(State::Acknowledged, Some("did the thing"), None).unwrap();
    assert_eq!(no_ev.state, State::Acted);
    assert_eq!(no_ev.warnings.len(), 1);
    assert_eq!(no_ev.warnings[0].kind, "lifecycle.evidence_missing");

    let blank = mark_acted(State::Acknowledged, None, Some("   ")).unwrap();
    assert_eq!(blank.warnings.len(), 1);

    let with_ev = mark_acted(
        State::Acknowledged,
        None,
        Some("https://github.com/acme/pr/1"),
    )
    .unwrap();
    assert_eq!(with_ev.state, State::Acted);
    assert!(with_ev.warnings.is_empty());

    // mark_acted from an illegal predecessor surfaces the transition error.
    assert!(mark_acted(State::Generated, None, None).is_err());
}

fn ts() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-05-15T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

/// Outcome window is Acted + 14d.
#[test]
fn outcome_window_is_14_days() {
    assert_eq!(OUTCOME_WINDOW_DAYS, 14);
    assert_eq!(outcome_due_at(ts()), ts() + Duration::days(14));
}

/// §6.3 outcome: improved / regressed / unchanged with enough runs;
/// inconclusive when post-Acted runs < MIN_POST_ACTED_RUNS regardless of delta.
#[test]
fn outcome_measurement_branches() {
    let measured = outcome_due_at(ts());

    let improved = measure_outcome(
        measured,
        "brand_docs_citation_count",
        0.0,
        3.0,
        MIN_POST_ACTED_RUNS,
    );
    assert_eq!(improved.status, OutcomeStatus::Improved);

    let regressed = measure_outcome(measured, "m", 5.0, 2.0, 12);
    assert_eq!(regressed.status, OutcomeStatus::Regressed);

    let unchanged = measure_outcome(measured, "m", 4.0, 4.0, 20);
    assert_eq!(unchanged.status, OutcomeStatus::Unchanged);

    // A huge improvement is still inconclusive with too few runs.
    let thin = measure_outcome(measured, "m", 0.0, 100.0, MIN_POST_ACTED_RUNS - 1);
    assert_eq!(thin.status, OutcomeStatus::Inconclusive);
}
