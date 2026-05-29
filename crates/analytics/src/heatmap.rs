//! Phase 2 Story 14.3 — visibility heatmap.
//!
//! A 2-D grid of `(date × provider)` cells where each cell carries the
//! brand's presence rate (fraction of prompt-runs where the brand was
//! ranked, 0..=1) and the average rank (None when no run had the brand
//! present). The dashboard renders this as a colored grid; the
//! per-cell payload also drives the architecture's accessibility
//! companion table (per UX-DR rule, every visual must have an
//! a11y-friendly tabular alternative).
//!
//! Pure function — no DB. Backend parity (ARCH-26a) falls out
//! automatically because the SQL only feeds the sample rows in; the
//! aggregation happens here.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One observation: a brand was (or wasn't) ranked on `date` by
/// `provider`. The caller pre-joins the prompt_runs + mentions tables
/// into this shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub date: NaiveDate,
    pub provider: String,
    /// `Some(rank)` if the brand appeared; `None` if it didn't.
    pub rank: Option<f64>,
}

/// One rendered cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatmapCell {
    pub date: NaiveDate,
    pub provider: String,
    /// Number of prompt-runs in this (date, provider) bucket. Helps the
    /// dashboard distinguish "very low presence on lots of runs"
    /// (signal) from "low presence on one run" (noise).
    pub runs: u32,
    /// Fraction of runs where the brand was present (`Some(rank)`).
    /// Always in [0, 1].
    pub presence_rate: f64,
    /// Average rank across present runs. `None` if no run had the
    /// brand present.
    pub avg_rank: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heatmap {
    pub cells: Vec<HeatmapCell>,
}

/// Group `samples` by `(date, provider)` and emit one cell per group.
/// Output is ordered (date ascending, provider lexicographic) so the
/// dashboard can render row/column headers deterministically and the
/// ARCH-26a parity test against ClickHouse can byte-compare.
pub fn compute(samples: &[Sample]) -> Heatmap {
    let mut buckets: BTreeMap<(NaiveDate, String), Vec<Option<f64>>> = BTreeMap::new();
    for sample in samples {
        buckets
            .entry((sample.date, sample.provider.clone()))
            .or_default()
            .push(sample.rank);
    }
    let mut cells = Vec::with_capacity(buckets.len());
    for ((date, provider), ranks) in buckets {
        let runs = ranks.len() as u32;
        let present: Vec<f64> = ranks.iter().filter_map(|r| *r).collect();
        let presence_rate = if runs == 0 {
            0.0
        } else {
            present.len() as f64 / runs as f64
        };
        let avg_rank = if present.is_empty() {
            None
        } else {
            Some(present.iter().sum::<f64>() / present.len() as f64)
        };
        cells.push(HeatmapCell {
            date,
            provider,
            runs,
            presence_rate,
            avg_rank,
        });
    }
    Heatmap { cells }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap()
    }

    fn sample(date: NaiveDate, provider: &str, rank: Option<f64>) -> Sample {
        Sample {
            date,
            provider: provider.to_string(),
            rank,
        }
    }

    #[test]
    fn empty_input_returns_empty_grid() {
        assert_eq!(compute(&[]).cells, Vec::<HeatmapCell>::new());
    }

    #[test]
    fn one_sample_one_cell() {
        let map = compute(&[sample(d(2026, 5, 28), "openai", Some(2.0))]);
        assert_eq!(map.cells.len(), 1);
        let cell = &map.cells[0];
        assert_eq!(cell.runs, 1);
        assert_eq!(cell.presence_rate, 1.0);
        assert_eq!(cell.avg_rank, Some(2.0));
    }

    #[test]
    fn cells_grouped_by_date_and_provider() {
        let map = compute(&[
            sample(d(2026, 5, 28), "openai", Some(2.0)),
            sample(d(2026, 5, 28), "openai", Some(3.0)),
            sample(d(2026, 5, 28), "anthropic", Some(1.0)),
            sample(d(2026, 5, 29), "openai", None),
        ]);
        assert_eq!(map.cells.len(), 3);

        // Order is (date asc, provider lex), so:
        // (5/28, anthropic), (5/28, openai), (5/29, openai)
        assert_eq!(map.cells[0].provider, "anthropic");
        assert_eq!(map.cells[0].avg_rank, Some(1.0));
        assert_eq!(map.cells[0].runs, 1);

        assert_eq!(map.cells[1].provider, "openai");
        assert_eq!(map.cells[1].runs, 2);
        assert_eq!(map.cells[1].avg_rank, Some(2.5));
        assert!((map.cells[1].presence_rate - 1.0).abs() < f64::EPSILON);

        assert_eq!(map.cells[2].date, d(2026, 5, 29));
        assert_eq!(map.cells[2].avg_rank, None);
        assert_eq!(map.cells[2].presence_rate, 0.0);
    }

    #[test]
    fn presence_rate_reflects_null_count_in_cell() {
        let map = compute(&[
            sample(d(2026, 5, 28), "openai", Some(2.0)),
            sample(d(2026, 5, 28), "openai", None),
            sample(d(2026, 5, 28), "openai", None),
            sample(d(2026, 5, 28), "openai", Some(3.0)),
        ]);
        let cell = &map.cells[0];
        assert_eq!(cell.runs, 4);
        assert!((cell.presence_rate - 0.5).abs() < f64::EPSILON);
        assert_eq!(cell.avg_rank, Some(2.5));
    }

    #[test]
    fn cell_order_is_stable_for_parity_test() {
        // ARCH-26a: Postgres + ClickHouse outputs must be byte-equal.
        // Same input → same cell ordering → same JSON bytes.
        let inputs = [
            sample(d(2026, 5, 28), "openai", Some(2.0)),
            sample(d(2026, 5, 27), "openai", Some(3.0)),
            sample(d(2026, 5, 28), "anthropic", Some(1.0)),
            sample(d(2026, 5, 27), "anthropic", None),
        ];
        let map1 = compute(&inputs);
        let map2 = compute(&inputs);
        assert_eq!(
            serde_json::to_vec(&map1).unwrap(),
            serde_json::to_vec(&map2).unwrap()
        );
        // Lex order by (date asc, provider asc):
        // 5/27 anthropic, 5/27 openai, 5/28 anthropic, 5/28 openai
        let dates: Vec<_> = map1.cells.iter().map(|c| c.date).collect();
        let providers: Vec<_> = map1.cells.iter().map(|c| c.provider.as_str()).collect();
        assert_eq!(
            dates,
            vec![d(2026, 5, 27), d(2026, 5, 27), d(2026, 5, 28), d(2026, 5, 28)]
        );
        assert_eq!(providers, vec!["anthropic", "openai", "anthropic", "openai"]);
    }

    #[test]
    fn all_null_cell_has_none_avg_rank() {
        let map = compute(&[
            sample(d(2026, 5, 28), "openai", None),
            sample(d(2026, 5, 28), "openai", None),
        ]);
        assert_eq!(map.cells[0].avg_rank, None);
        assert_eq!(map.cells[0].presence_rate, 0.0);
        assert_eq!(map.cells[0].runs, 2);
    }

    #[test]
    fn round_trip_through_serde_preserves_shape() {
        let map = compute(&[
            sample(d(2026, 5, 28), "openai", Some(2.0)),
            sample(d(2026, 5, 28), "anthropic", None),
        ]);
        let bytes = serde_json::to_vec(&map).unwrap();
        let back: Heatmap = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back, map);
    }
}
