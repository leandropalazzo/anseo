//! Phase 2 Story 14.4 — visibility-ranking volatility metric.
//!
//! Definition: coefficient of variation (CV) of a brand's ranking over
//! a rolling window. `CV = stddev / mean`, normalized into `[0, 1]` by
//! clamping at 1.0 (a fully-chaotic ranking series with CV > 1 hits the
//! ceiling — the analytics surface only needs the relative spread, not
//! the precise high-end value).
//!
//! Empty or all-null windows return [`Volatility::Absent`]; a single
//! observation gives CV = 0 (no spread); a constant non-null series
//! also gives CV = 0. Null observations are excluded from the mean +
//! stddev computation but tracked separately for the
//! `presence_ratio` field so callers can distinguish "stable, but the
//! brand often disappears" from "stable and always present".
//!
//! Wire shape matches the test design's expected dashboard surface:
//! `{ value: f64 in [0,1], presence_ratio: f64 in [0,1], samples: u32 }`.
//!
//! Pure function — no IO. Backend-parity (Postgres vs ClickHouse) for
//! Story 14.1 falls out automatically because the SQL only feeds in the
//! sample slice; the math here doesn't care about the source.

use serde::{Deserialize, Serialize};

/// Default rolling-window size in samples (14 daily observations →
/// two weeks at one prompt-run-per-day cadence).
pub const DEFAULT_WINDOW_SAMPLES: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Volatility {
    /// CV clamped to `[0, 1]`. 0 = perfectly stable; 1 = at-or-beyond
    /// the chaotic ceiling. `None` (serialized as JSON null) means the
    /// window has zero non-null samples — the brand wasn't present at
    /// all within the window.
    pub value: Option<f64>,
    /// Fraction of window samples that were non-null. 0.0 means the
    /// brand was never present; 1.0 means every observation was non-null.
    pub presence_ratio: f64,
    /// Window sample count fed in. Lets the dashboard surface "based on
    /// N samples" so an operator can distinguish a high-volatility
    /// reading on 14 samples from one on 3.
    pub samples: u32,
}

impl Volatility {
    /// Shorthand used by the tests + the API handler's empty-window
    /// branch.
    pub const ABSENT_FROM_ZERO_WINDOW: Self = Self {
        value: None,
        presence_ratio: 0.0,
        samples: 0,
    };
}

/// Compute the volatility metric for one window of ranking samples.
/// `samples` is a slice of `Option<f64>` where `None` means the brand
/// was not present in that observation.
pub fn compute(samples: &[Option<f64>]) -> Volatility {
    let total = samples.len() as u32;
    if total == 0 {
        return Volatility::ABSENT_FROM_ZERO_WINDOW;
    }
    let observed: Vec<f64> = samples.iter().filter_map(|s| *s).collect();
    let present = observed.len() as u32;
    let presence_ratio = present as f64 / total as f64;
    if present == 0 {
        return Volatility {
            value: None,
            presence_ratio,
            samples: total,
        };
    }
    if present == 1 {
        // Degenerate case: a single observation has zero spread by
        // definition. The presence_ratio carries the "rare" signal.
        return Volatility {
            value: Some(0.0),
            presence_ratio,
            samples: total,
        };
    }
    let n = observed.len() as f64;
    let mean = observed.iter().sum::<f64>() / n;
    let variance: f64 = observed
        .iter()
        .map(|x| {
            let d = x - mean;
            d * d
        })
        .sum::<f64>()
        / n;
    let stddev = variance.sqrt();
    let cv = if mean.abs() < f64::EPSILON {
        // All-zero ranks (unlikely — rank 1 is top) → no relative spread.
        0.0
    } else {
        stddev / mean.abs()
    };
    Volatility {
        value: Some(cv.clamp(0.0, 1.0)),
        presence_ratio,
        samples: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(values: &[Option<f64>]) -> &[Option<f64>] {
        values
    }

    #[test]
    fn empty_window_returns_absent() {
        let v = compute(s(&[]));
        assert_eq!(v, Volatility::ABSENT_FROM_ZERO_WINDOW);
    }

    #[test]
    fn all_null_window_returns_none_value_zero_presence() {
        let v = compute(s(&[None, None, None]));
        assert_eq!(v.value, None);
        assert_eq!(v.presence_ratio, 0.0);
        assert_eq!(v.samples, 3);
    }

    #[test]
    fn single_observation_has_zero_cv() {
        let v = compute(s(&[Some(2.0)]));
        assert_eq!(v.value, Some(0.0));
        assert_eq!(v.samples, 1);
        assert!((v.presence_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn constant_rank_has_zero_cv() {
        let v = compute(s(&[Some(2.0), Some(2.0), Some(2.0), Some(2.0)]));
        assert_eq!(v.value, Some(0.0));
        assert!((v.presence_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn alternating_high_low_has_nonzero_cv() {
        let v = compute(s(&[Some(1.0), Some(10.0), Some(1.0), Some(10.0)]));
        let value = v.value.expect("present samples");
        assert!(value > 0.5, "alternating 1↔10 should be highly volatile, got {value}");
        assert!(value <= 1.0, "value must be clamped to ≤ 1.0");
    }

    #[test]
    fn extreme_swing_clamps_at_one() {
        // Asymmetric distribution where stddev exceeds the mean —
        // e.g. mostly-1 with rare high spikes pulls the mean low but
        // leaves the stddev large, pushing CV above 1.
        let mut samples = vec![Some(1.0); 9];
        samples.push(Some(1000.0));
        let v = compute(&samples);
        assert_eq!(v.value, Some(1.0));
    }

    #[test]
    fn cv_below_one_passes_through_uncapped() {
        // Sanity-check the symmetric case: two values around a positive
        // mean give a CV < 1 that should pass through without clamping.
        let v = compute(s(&[Some(1.0), Some(100.0)]));
        let value = v.value.expect("present samples");
        assert!(value < 1.0, "two-point symmetric CV should be < 1, got {value}");
        assert!(value > 0.9);
    }

    #[test]
    fn presence_ratio_tracks_null_count() {
        // 2 of 4 samples are nulls — presence_ratio should be 0.5.
        let v = compute(s(&[Some(2.0), None, Some(3.0), None]));
        assert!((v.presence_ratio - 0.5).abs() < f64::EPSILON);
        assert!(v.value.is_some());
    }

    #[test]
    fn one_observation_with_nulls_still_zero_cv() {
        let v = compute(s(&[None, Some(2.0), None]));
        assert_eq!(v.value, Some(0.0));
        assert!((v.presence_ratio - 1.0 / 3.0).abs() < f64::EPSILON);
        assert_eq!(v.samples, 3);
    }

    #[test]
    fn nulls_excluded_from_mean_stddev() {
        // Without nulls: [2,3,4,5] → mean 3.5, stddev ~1.118, cv ~0.32
        let with_null = compute(s(&[Some(2.0), Some(3.0), None, Some(4.0), Some(5.0)]));
        let without_null = compute(s(&[Some(2.0), Some(3.0), Some(4.0), Some(5.0)]));
        // Values should match (nulls excluded from math); presence_ratio
        // is the only field that differs.
        assert_eq!(with_null.value, without_null.value);
        assert_ne!(with_null.presence_ratio, without_null.presence_ratio);
    }

    #[test]
    fn default_window_matches_anomaly_detector_default() {
        // Pin the cross-module consistency: anomaly + volatility should
        // use the same 14-sample default so dashboards built on either
        // metric share the same context window.
        use opengeo_core::AnomalySensitivity;
        let anomaly_default = AnomalySensitivity::default();
        assert_eq!(DEFAULT_WINDOW_SAMPLES, anomaly_default.window_samples);
    }

    #[test]
    fn value_field_serializes_to_null_when_absent() {
        // Wire-shape pin: the dashboard reads `.value` and renders a
        // "—" placeholder for null. A future serde-rename refactor must
        // not silently change this.
        let v = compute(s(&[]));
        let json = serde_json::to_value(&v).unwrap();
        assert!(json["value"].is_null());
        assert_eq!(json["samples"], 0);
    }

    #[test]
    fn round_trip_through_serde_preserves_shape() {
        let v = compute(s(&[Some(1.0), Some(2.0), None, Some(3.0)]));
        let bytes = serde_json::to_vec(&v).unwrap();
        let back: Volatility = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, v);
    }
}
