//! Z-score visibility detector for FR-26a.
//!
//! Stream: a per-Provider time-ordered list of ranking samples. `rank = None`
//! means the brand was absent from that Provider's answer for that Prompt
//! Run. The detector emits a verdict in two cases:
//!
//! 1. A null observation following at least `window_samples` non-null
//!    observations whose stddev is small (the brand was reliably present
//!    and now disappeared). Wire `summary = "rank_disappeared"`.
//! 2. A numeric observation whose z-score against the trailing window
//!    exceeds `threshold`. Wire `summary = "z=…, prev=mean±stddev"`.
//!
//! The detector is intentionally simple — no LLM, no derivatives, no
//! seasonality. Phase 3 may add a pluggable detector behind the same
//! `AnomalyVerdict` shape; FR-26a's Phase 2 budget is z-score only.

use super::{AnomalyKind, AnomalyVerdict};
use chrono::{DateTime, Utc};
use opengeo_core::ProviderName;
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RankSample {
    pub observed_at: DateTime<Utc>,
    pub provider: ProviderName,
    /// Rank position (1 = top). `None` means not present in the answer.
    pub rank: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub zscore_threshold: f64,
    pub window_samples: usize,
}

/// Scan `samples` chronologically and emit one verdict per anomalous point.
///
/// **Precondition (load-bearing):** `samples` must come from a single
/// Provider. The trailing-window stddev would be meaningless across
/// Providers because each Provider has its own noise distribution. The
/// public `super::detect_visibility` enforces this by grouping; this
/// function is intentionally `detect_visibility_single_provider` at the
/// crate root.
///
/// Samples are assumed pre-sorted by `observed_at`; the detector does not
/// re-sort because the caller's SQL ORDER BY is the authoritative ordering.
///
/// Repeated anomalies self-suppress: when a single outlier fires a z-score
/// verdict, the *next* sample's window now contains that outlier, so the
/// window stddev widens and a normal-range follow-up does not re-fire.
/// A sustained absence similarly stops emitting after the first verdict
/// because the dense-window precondition no longer holds.
pub fn detect(samples: &[RankSample], cfg: Config) -> Vec<AnomalyVerdict> {
    let mut verdicts = Vec::new();
    if samples.len() <= cfg.window_samples {
        return verdicts;
    }
    for i in cfg.window_samples..samples.len() {
        let window = &samples[i - cfg.window_samples..i];
        let current = &samples[i];

        match current.rank {
            None => {
                if let Some(verdict) = disappearance_verdict(window, current) {
                    verdicts.push(verdict);
                }
            }
            Some(rank_value) => {
                if let Some(verdict) =
                    zscore_verdict(window, current, rank_value, cfg.zscore_threshold)
                {
                    verdicts.push(verdict);
                }
            }
        }
    }
    verdicts
}

fn disappearance_verdict(
    window: &[RankSample],
    current: &RankSample,
) -> Option<AnomalyVerdict> {
    // Only flag a disappearance when the trailing window is dense with
    // non-null observations — a brand that's typically absent does not
    // become noteworthy each time the absence repeats.
    let present_count = window.iter().filter(|s| s.rank.is_some()).count();
    if present_count < window.len() {
        return None;
    }
    let mean = window
        .iter()
        .filter_map(|s| s.rank)
        .sum::<f64>()
        / window.len() as f64;
    Some(AnomalyVerdict {
        kind: AnomalyKind::Visibility,
        observed_at: current.observed_at,
        provider: current.provider,
        summary: format!("rank_disappeared (prev_mean={mean:.2})"),
        detail: json!({
            "signal": "rank_disappeared",
            "prev_mean": mean,
            "window_samples": window.len(),
        }),
    })
}

fn zscore_verdict(
    window: &[RankSample],
    current: &RankSample,
    rank_value: f64,
    threshold: f64,
) -> Option<AnomalyVerdict> {
    let observed: Vec<f64> = window.iter().filter_map(|s| s.rank).collect();
    if observed.is_empty() {
        // A first-time appearance after sustained absence — flag it.
        return Some(AnomalyVerdict {
            kind: AnomalyKind::Visibility,
            observed_at: current.observed_at,
            provider: current.provider,
            summary: format!("first_appearance (rank={rank_value:.1})"),
            detail: json!({
                "signal": "first_appearance",
                "rank": rank_value,
            }),
        });
    }
    let mean = observed.iter().sum::<f64>() / observed.len() as f64;
    let variance: f64 = observed
        .iter()
        .map(|x| (x - mean).powi(2))
        .sum::<f64>()
        / observed.len() as f64;
    let stddev = variance.sqrt();

    if stddev < 1e-9 {
        // Window is degenerate (constant rank). Any change is an anomaly,
        // but a one-step neighboring value (e.g., 2 → 3) shouldn't trip;
        // require a delta of at least 2 ranks before flagging.
        if (rank_value - mean).abs() >= 2.0 {
            return Some(AnomalyVerdict {
                kind: AnomalyKind::Visibility,
                observed_at: current.observed_at,
                provider: current.provider,
                summary: format!(
                    "constant_window_shift (rank={rank_value:.1}, prev_const={mean:.1})"
                ),
                detail: json!({
                    "signal": "constant_window_shift",
                    "rank": rank_value,
                    "prev_constant": mean,
                }),
            });
        }
        return None;
    }

    let z = (rank_value - mean) / stddev;
    if z.abs() < threshold {
        return None;
    }
    Some(AnomalyVerdict {
        kind: AnomalyKind::Visibility,
        observed_at: current.observed_at,
        provider: current.provider,
        summary: format!(
            "z={z:.2}, rank={rank_value:.1}, prev={mean:.2}±{stddev:.2}"
        ),
        detail: json!({
            "signal": "zscore",
            "z": z,
            "rank": rank_value,
            "window_mean": mean,
            "window_stddev": stddev,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample(day: u32, rank: Option<f64>) -> RankSample {
        RankSample {
            observed_at: Utc.with_ymd_and_hms(2026, 5, day, 12, 0, 0).unwrap(),
            provider: ProviderName::Openai,
            rank,
        }
    }

    fn cfg() -> Config {
        Config {
            zscore_threshold: 2.5,
            window_samples: 7,
        }
    }

    #[test]
    fn no_verdicts_when_history_too_short() {
        let series = (1..=5).map(|d| sample(d, Some(2.0))).collect::<Vec<_>>();
        assert!(detect(&series, cfg()).is_empty());
    }

    #[test]
    fn constant_rank_then_disappearance_emits_visibility_verdict() {
        let mut series: Vec<RankSample> = (1..=8).map(|d| sample(d, Some(2.0))).collect();
        series.push(sample(9, None));
        let verdicts = detect(&series, cfg());
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].kind, AnomalyKind::Visibility);
        assert!(verdicts[0].summary.starts_with("rank_disappeared"));
    }

    #[test]
    fn stable_rank_with_one_neighboring_drift_is_quiet() {
        // Rank 2.0 for 7 days, then 3.0 — under stddev=0 the 1-step shift
        // is NOT an anomaly (anti-noise floor).
        let mut series: Vec<RankSample> = (1..=7).map(|d| sample(d, Some(2.0))).collect();
        series.push(sample(8, Some(3.0)));
        let verdicts = detect(&series, cfg());
        assert!(verdicts.is_empty(), "{verdicts:?}");
    }

    #[test]
    fn stable_rank_with_large_shift_under_zero_stddev_emits_verdict() {
        // Rank 2.0 for 7 days, then 8.0 — delta >= 2 ranks under stddev=0
        // tripp the constant_window_shift path.
        let mut series: Vec<RankSample> = (1..=7).map(|d| sample(d, Some(2.0))).collect();
        series.push(sample(8, Some(8.0)));
        let verdicts = detect(&series, cfg());
        assert_eq!(verdicts.len(), 1);
        assert!(verdicts[0].summary.starts_with("constant_window_shift"));
    }

    #[test]
    fn varying_rank_within_threshold_is_quiet() {
        // Rank wanders 1..4; the spread is enough that 3.0 -> 4.0 isn't
        // 2.5 sigma away.
        let series = vec![
            sample(1, Some(2.0)),
            sample(2, Some(3.0)),
            sample(3, Some(2.0)),
            sample(4, Some(4.0)),
            sample(5, Some(3.0)),
            sample(6, Some(2.0)),
            sample(7, Some(3.0)),
            sample(8, Some(4.0)),
        ];
        let verdicts = detect(&series, cfg());
        assert!(verdicts.is_empty(), "{verdicts:?}");
    }

    #[test]
    fn varying_rank_with_outlier_emits_zscore_verdict() {
        let series = vec![
            sample(1, Some(2.0)),
            sample(2, Some(3.0)),
            sample(3, Some(2.0)),
            sample(4, Some(3.0)),
            sample(5, Some(2.0)),
            sample(6, Some(3.0)),
            sample(7, Some(2.0)),
            sample(8, Some(20.0)),
        ];
        let verdicts = detect(&series, cfg());
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].detail["signal"], "zscore");
    }

    #[test]
    fn synthetic_year_long_stable_stream_stays_under_annual_budget() {
        // FR-26a P1-103 calibration: a year of stable observations at the
        // *project default* sensitivity (window 14, threshold 2.5) must
        // emit ≤ 12 visibility anomalies. The 7-sample window the other
        // tests use is intentionally tight; the project default is 14
        // because the calibration target needs the wider window's more
        // stable stddev estimate.
        let project_default = Config {
            zscore_threshold: 2.5,
            window_samples: 14,
        };
        let mut rng_state: u64 = 0xDEAD_BEEF;
        let mut series = Vec::new();
        for d in 0..365 {
            rng_state = rng_state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let noise = ((rng_state >> 32) & 0xFFFF) as f64 / 65536.0; // [0,1)
            let rank = 2.0 + (noise - 0.5) * 0.4;
            series.push(RankSample {
                observed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
                    + chrono::Duration::days(d),
                provider: ProviderName::Openai,
                rank: Some(rank),
            });
        }
        let verdicts = detect(&series, project_default);
        assert!(
            verdicts.len() <= 12,
            "stable stream produced {} anomalies; FR-26a budget is ≤ 12/year",
            verdicts.len()
        );
    }
}
