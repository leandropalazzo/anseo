//! Retry ladder for Phase 2 webhook delivery (architecture §5 / C-7).
//!
//! Canonical schedule: **1s, 30s, 5min, 1h, 6h**. After the 5th attempt
//! (the final 6-hour wait expires) the delivery is marked
//! `failed_permanent`. After 5 consecutive permanent-failed deliveries on
//! the same webhook target, the webhook auto-disables (architecture §5.4);
//! that escalation is a separate concern, handled by the dispatcher.
//!
//! Pure functions only — no clock, no IO. The dispatcher feeds in the
//! attempt count and gets back the next wait Duration (or `None`,
//! meaning "give up").

use std::time::Duration;

/// The canonical 5-step retry ladder, in order. Attempt 1 fires
/// immediately on the source event; attempt 2 waits LADDER[0] (1s) before
/// firing; attempt 6 never happens (the function returns None for any
/// `attempt >= LADDER.len() + 1`).
pub const LADDER: &[Duration] = &[
    Duration::from_secs(1),
    Duration::from_secs(30),
    Duration::from_secs(5 * 60),
    Duration::from_secs(60 * 60),
    Duration::from_secs(6 * 60 * 60),
];

/// Maximum attempts before a delivery is `failed_permanent`. The first
/// attempt is "attempt 1"; the 5 ladder entries are the waits between
/// attempts 1→2, 2→3, …, 5→6. After attempt 5 fails there is no 6th try.
pub const MAX_ATTEMPTS: u32 = 5;

/// Threshold for auto-disabling a webhook target: 5 consecutive
/// permanent-failed deliveries (architecture §5.4).
pub const AUTO_DISABLE_THRESHOLD: u32 = 5;

/// Return the duration to wait before the next attempt, given the number
/// of attempts already made. Returns `None` when no further attempt is
/// allowed (the dispatcher must mark the delivery `failed_permanent`).
///
/// Examples (`attempts_so_far` is what was already tried, NOT the next
/// attempt number):
/// - 0 → first attempt: caller fires immediately (this function isn't on
///   the first-fire path; returns Some(0) for completeness so the
///   dispatcher can use a single uniform loop).
/// - 1 → wait 1s before attempt 2
/// - 2 → wait 30s before attempt 3
/// - 3 → wait 5min before attempt 4
/// - 4 → wait 1h before attempt 5
/// - 5 → no further attempt; returns None
pub fn next_delay(attempts_so_far: u32) -> Option<Duration> {
    if attempts_so_far == 0 {
        return Some(Duration::from_secs(0));
    }
    if attempts_so_far >= MAX_ATTEMPTS {
        return None;
    }
    LADDER.get(attempts_so_far as usize - 1).copied()
}

/// True when the delivery has exhausted the retry ladder and the next
/// state transition should be `failed_permanent`.
pub fn is_exhausted(attempts_so_far: u32) -> bool {
    attempts_so_far >= MAX_ATTEMPTS
}

/// True when consecutive permanent-failed deliveries on the same webhook
/// have hit the auto-disable threshold.
pub fn should_auto_disable(consecutive_permanent_failures: u32) -> bool {
    consecutive_permanent_failures >= AUTO_DISABLE_THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_matches_arch_5() {
        // Pinned per architecture §5: 1s, 30s, 5min, 1h, 6h.
        assert_eq!(LADDER.len(), 5);
        assert_eq!(LADDER[0], Duration::from_secs(1));
        assert_eq!(LADDER[1], Duration::from_secs(30));
        assert_eq!(LADDER[2], Duration::from_secs(300));
        assert_eq!(LADDER[3], Duration::from_secs(3600));
        assert_eq!(LADDER[4], Duration::from_secs(21_600));
    }

    #[test]
    fn next_delay_zero_returns_zero_for_first_fire() {
        assert_eq!(next_delay(0), Some(Duration::from_secs(0)));
    }

    #[test]
    fn next_delay_walks_the_full_ladder() {
        // After attempt 1: wait 1s before attempt 2.
        assert_eq!(next_delay(1), Some(Duration::from_secs(1)));
        assert_eq!(next_delay(2), Some(Duration::from_secs(30)));
        assert_eq!(next_delay(3), Some(Duration::from_secs(300)));
        assert_eq!(next_delay(4), Some(Duration::from_secs(3600)));
    }

    #[test]
    fn next_delay_returns_none_after_5_attempts() {
        assert_eq!(next_delay(5), None);
        assert_eq!(next_delay(6), None);
        assert_eq!(next_delay(100), None);
    }

    #[test]
    fn is_exhausted_flips_at_max() {
        assert!(!is_exhausted(0));
        assert!(!is_exhausted(4));
        assert!(is_exhausted(5));
        assert!(is_exhausted(6));
    }

    #[test]
    fn auto_disable_threshold_matches_arch_5_4() {
        assert_eq!(AUTO_DISABLE_THRESHOLD, 5);
        assert!(!should_auto_disable(0));
        assert!(!should_auto_disable(4));
        assert!(should_auto_disable(5));
        assert!(should_auto_disable(6));
    }

    #[test]
    fn ladder_is_monotonically_increasing() {
        // A regression in this property would mean a future tweak shipped
        // a faster retry than the previous step — operators rely on the
        // ladder spacing out aggressively to keep upstream load bounded.
        for window in LADDER.windows(2) {
            assert!(
                window[1] > window[0],
                "retry ladder must be monotonically increasing, got {:?} -> {:?}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn total_window_under_eight_hours() {
        // Pin the total maximum delay between event and final attempt
        // (~7h ish). Architecture promises "permanently failed within
        // ~one operator workday" — anything over 8h breaks the SLA.
        let total: Duration = LADDER.iter().sum();
        assert!(
            total < Duration::from_secs(8 * 3600),
            "total ladder window {} exceeds 8h SLA",
            total.as_secs()
        );
    }
}
