//! FR-26a — statistical anomaly detection over Ranking + Citation streams.
//!
//! Two independent detectors:
//!
//! - [`zscore`] watches numeric Ranking series (1 = top, higher = lower
//!   visibility, `None` = not present). A new observation whose z-score
//!   (against the trailing `window_samples` history) exceeds the configured
//!   `zscore_threshold` emits a `visibility_anomaly` verdict. A transition
//!   from non-null to null in an otherwise stable stream is treated as a
//!   visibility anomaly as well — disappearing from a Provider's answers is
//!   a load-bearing signal.
//!
//! - [`citation_novelty`] watches per-prompt Citation source-domain sets. A
//!   domain that does not appear in any of the historical windows but
//!   appears `citation_min_frequency` or more times in the current sample
//!   emits a `citation_anomaly` verdict.
//!
//! Both detectors are pure functions over already-fetched data: callers
//! pull the time series from the MetricsStore (Postgres in Phase 2,
//! ClickHouse opt-in in Story 14.1) and feed the slice in. This keeps the
//! detector itself backend-agnostic and lets the Story 14.1 parity test
//! prove ARCH-26a by feeding the same slice from both backends and asserting
//! the verdict set matches byte-for-byte.

pub mod citation_novelty;
pub mod zscore;

use anseo_core::ProviderName;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Wire-stable taxonomy for anomaly verdicts. Maps onto the ARCH-17 event
/// kinds (`visibility.anomaly`, `citation.anomaly`) when the worker fans
/// out detector output onto the lifecycle event channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyKind {
    Visibility,
    Citation,
}

/// One detector verdict. Independent of the SSE / webhook fanout: the
/// scheduler maps verdicts onto `LifecycleEvent::VisibilityAnomaly` /
/// `CitationAnomaly` for fanout but the detector itself only emits the
/// signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnomalyVerdict {
    pub kind: AnomalyKind,
    pub observed_at: DateTime<Utc>,
    pub provider: ProviderName,
    /// One-line operator summary (e.g., "z=3.4, prev=2.1±0.3").
    pub summary: String,
    /// Structured detail blob carried through to subscribers verbatim.
    pub detail: serde_json::Value,
}

pub use citation_novelty::{detect as detect_citation_novelty_single_provider, CitationSample};
pub use zscore::{detect as detect_visibility_single_provider, RankSample};

/// Run the z-score detector across a mixed-Provider sample stream.
///
/// Both detectors compute statistics over the trailing window of samples;
/// mixing samples from multiple Providers would let one Provider's noise
/// distribution corrupt another's. This helper groups by Provider (stable
/// order by `ProviderName`'s wire string) and dispatches to the per-Provider
/// `detect` so callers cannot mis-use the underlying detectors.
pub fn detect_visibility(samples: &[RankSample], cfg: zscore::Config) -> Vec<AnomalyVerdict> {
    let mut by_provider: std::collections::BTreeMap<String, Vec<RankSample>> =
        std::collections::BTreeMap::new();
    for s in samples {
        by_provider
            .entry(s.provider.as_wire_str().into_owned())
            .or_default()
            .push(s.clone());
    }
    let mut all = Vec::new();
    for series in by_provider.values() {
        all.extend(zscore::detect(series, cfg));
    }
    all
}

/// Run the citation-novelty detector across a mixed-Provider stream. Same
/// grouping rationale as [`detect_visibility`]: a Phase 1 Provider's known
/// citation domains must not censor a Phase 2 Provider's first appearance.
pub fn detect_citation_novelty(
    samples: &[CitationSample],
    cfg: citation_novelty::Config,
) -> Vec<AnomalyVerdict> {
    let mut by_provider: std::collections::BTreeMap<String, Vec<CitationSample>> =
        std::collections::BTreeMap::new();
    for s in samples {
        by_provider
            .entry(s.provider.as_wire_str().into_owned())
            .or_default()
            .push(s.clone());
    }
    let mut all = Vec::new();
    for series in by_provider.values() {
        all.extend(citation_novelty::detect(series, cfg));
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use anseo_core::ProviderName;
    use chrono::{TimeZone, Utc};

    #[test]
    fn visibility_grouping_isolates_per_provider_statistics() {
        // Two Providers interleaved on the same timeline: OpenAI is
        // stable at 2.0, Anthropic is stable at 8.0. A mixed-window
        // stddev would be huge (~3.0), and the OpenAI day-8 sample of
        // 2.0 would not look anomalous — but Anthropic day-8 jumping
        // to 1.0 should fire.
        let mut samples = Vec::new();
        for d in 1..=8 {
            samples.push(RankSample {
                observed_at: Utc.with_ymd_and_hms(2026, 5, d, 12, 0, 0).unwrap(),
                provider: ProviderName::Openai,
                rank: Some(2.0),
            });
            samples.push(RankSample {
                observed_at: Utc.with_ymd_and_hms(2026, 5, d, 12, 0, 0).unwrap(),
                provider: ProviderName::Anthropic,
                rank: Some(if d < 8 { 8.0 } else { 1.0 }),
            });
        }
        let cfg = zscore::Config {
            zscore_threshold: 2.5,
            window_samples: 7,
        };
        let verdicts = detect_visibility(&samples, cfg);
        assert_eq!(
            verdicts.len(),
            1,
            "expected exactly one Anthropic verdict, got: {verdicts:?}"
        );
        assert_eq!(verdicts[0].provider, ProviderName::Anthropic);
    }

    #[test]
    fn citation_novelty_grouping_isolates_per_provider_known_set() {
        let mut samples = Vec::new();
        for d in 1..=7 {
            samples.push(CitationSample {
                observed_at: Utc.with_ymd_and_hms(2026, 5, d, 12, 0, 0).unwrap(),
                provider: ProviderName::Openai,
                domains: vec!["openai-docs.io".into()],
            });
            samples.push(CitationSample {
                observed_at: Utc.with_ymd_and_hms(2026, 5, d, 12, 0, 0).unwrap(),
                provider: ProviderName::Anthropic,
                domains: vec!["anthropic-docs.io".into()],
            });
        }
        // Day 8: each Provider sees the *other* Provider's known domain
        // for the first time. With grouping isolation, both fire.
        // Without grouping, neither would fire.
        samples.push(CitationSample {
            observed_at: Utc.with_ymd_and_hms(2026, 5, 8, 12, 0, 0).unwrap(),
            provider: ProviderName::Openai,
            domains: vec!["anthropic-docs.io".into(), "anthropic-docs.io".into()],
        });
        samples.push(CitationSample {
            observed_at: Utc.with_ymd_and_hms(2026, 5, 8, 12, 0, 0).unwrap(),
            provider: ProviderName::Anthropic,
            domains: vec!["openai-docs.io".into(), "openai-docs.io".into()],
        });
        let cfg = citation_novelty::Config {
            window_samples: 7,
            min_frequency: 2,
        };
        let verdicts = detect_citation_novelty(&samples, cfg);
        assert_eq!(verdicts.len(), 2);
    }
}
