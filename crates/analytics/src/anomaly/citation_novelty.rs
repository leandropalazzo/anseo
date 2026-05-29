//! Citation-novelty detector for FR-26a.
//!
//! Watches the set of distinct citation source-domains observed per Provider.
//! A domain absent from the trailing `window_samples` history but appearing
//! with frequency ≥ `min_frequency` in the current sample emits a
//! `citation_anomaly` verdict. "Frequency" here means how many citations in
//! the current sample landed on that domain — `min_frequency = 2` filters
//! out single-occurrence noise.

use super::{AnomalyKind, AnomalyVerdict};
use chrono::{DateTime, Utc};
use opengeo_core::ProviderName;
use serde_json::json;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct CitationSample {
    pub observed_at: DateTime<Utc>,
    pub provider: ProviderName,
    /// Source domains seen in this sample (one entry per citation,
    /// duplicates count toward frequency).
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub window_samples: usize,
    pub min_frequency: u32,
}

/// Scan `samples` chronologically; for each sample after the warm-up
/// window, emit one verdict per newly-introduced high-frequency domain.
///
/// **Precondition (load-bearing):** `samples` must come from a single
/// Provider. The "previously seen" domain set is per-Provider: a domain
/// known to OpenAI must not censor a first appearance for Anthropic.
/// The public `super::detect_citation_novelty` enforces this by grouping;
/// this function is intentionally `detect_citation_novelty_single_provider`
/// at the crate root.
pub fn detect(samples: &[CitationSample], cfg: Config) -> Vec<AnomalyVerdict> {
    let mut verdicts = Vec::new();
    if samples.len() <= cfg.window_samples {
        return verdicts;
    }

    for i in cfg.window_samples..samples.len() {
        let history = &samples[i - cfg.window_samples..i];
        let current = &samples[i];

        let mut known: HashSet<&str> = HashSet::new();
        for past in history {
            for d in &past.domains {
                known.insert(d.as_str());
            }
        }

        let mut current_freq: HashMap<&str, u32> = HashMap::new();
        for d in &current.domains {
            *current_freq.entry(d.as_str()).or_insert(0) += 1;
        }

        let mut new_high_freq: Vec<(&str, u32)> = current_freq
            .iter()
            .filter(|(d, count)| !known.contains(*d) && **count >= cfg.min_frequency)
            .map(|(d, c)| (*d, *c))
            .collect();
        // Stable verdict order across runs — important for the ARCH-26a
        // parity test that asserts Postgres + ClickHouse produce
        // byte-equal output.
        new_high_freq.sort_by(|a, b| a.0.cmp(b.0));

        for (domain, freq) in new_high_freq {
            verdicts.push(AnomalyVerdict {
                kind: AnomalyKind::Citation,
                observed_at: current.observed_at,
                provider: current.provider,
                summary: format!("new_domain={domain} freq={freq}"),
                detail: json!({
                    "signal": "new_high_frequency_domain",
                    "domain": domain,
                    "frequency": freq,
                }),
            });
        }
    }
    verdicts
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample(day: u32, domains: &[&str]) -> CitationSample {
        CitationSample {
            observed_at: Utc.with_ymd_and_hms(2026, 5, day, 12, 0, 0).unwrap(),
            provider: ProviderName::Openai,
            domains: domains.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn cfg() -> Config {
        Config {
            window_samples: 7,
            min_frequency: 2,
        }
    }

    #[test]
    fn no_verdicts_in_warm_up_window() {
        let series = vec![
            sample(1, &["a.com", "a.com"]),
            sample(2, &["a.com"]),
            sample(3, &["b.com", "b.com"]),
        ];
        assert!(detect(&series, cfg()).is_empty());
    }

    #[test]
    fn previously_unseen_high_freq_domain_emits_verdict() {
        let mut series: Vec<CitationSample> = (1..=7)
            .map(|d| sample(d, &["docs.example.com", "docs.example.com"]))
            .collect();
        series.push(sample(8, &["new.io", "new.io", "new.io", "docs.example.com"]));
        let verdicts = detect(&series, cfg());
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].kind, AnomalyKind::Citation);
        assert!(verdicts[0].summary.contains("new.io"));
        assert_eq!(verdicts[0].detail["frequency"], 3);
    }

    #[test]
    fn single_appearance_of_new_domain_is_quiet() {
        let mut series: Vec<CitationSample> =
            (1..=7).map(|d| sample(d, &["a.com"])).collect();
        series.push(sample(8, &["a.com", "rare.io"]));
        let verdicts = detect(&series, cfg());
        assert!(verdicts.is_empty(), "{verdicts:?}");
    }

    #[test]
    fn known_domain_does_not_emit_even_at_higher_frequency() {
        let mut series: Vec<CitationSample> =
            (1..=7).map(|d| sample(d, &["a.com"])).collect();
        series.push(sample(8, &["a.com", "a.com", "a.com", "a.com"]));
        let verdicts = detect(&series, cfg());
        assert!(verdicts.is_empty(), "{verdicts:?}");
    }

    #[test]
    fn verdict_order_is_stable_for_parity_test() {
        // ARCH-26a — Postgres + ClickHouse outputs must be byte-equal.
        let mut series: Vec<CitationSample> =
            (1..=7).map(|d| sample(d, &["a.com"])).collect();
        series.push(sample(
            8,
            &["zeta.io", "zeta.io", "alpha.io", "alpha.io", "mu.io", "mu.io"],
        ));
        let verdicts = detect(&series, cfg());
        assert_eq!(verdicts.len(), 3);
        let domains: Vec<&serde_json::Value> =
            verdicts.iter().map(|v| &v.detail["domain"]).collect();
        assert_eq!(domains, vec!["alpha.io", "mu.io", "zeta.io"]);
    }

    #[test]
    fn provider_field_is_carried_through() {
        let mut series: Vec<CitationSample> =
            (1..=7).map(|d| sample(d, &["a.com"])).collect();
        series.push(CitationSample {
            observed_at: Utc.with_ymd_and_hms(2026, 5, 8, 12, 0, 0).unwrap(),
            provider: ProviderName::Anthropic,
            domains: vec!["new.io".into(), "new.io".into()],
        });
        let verdicts = detect(&series, cfg());
        assert_eq!(verdicts.len(), 1);
        assert_eq!(verdicts[0].provider, ProviderName::Anthropic);
    }
}
