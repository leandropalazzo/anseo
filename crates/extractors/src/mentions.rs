//! Mention extraction + ranking (FR-3, FR-5).
//!
//! Case-insensitive substring scan for the brand and every competitor (plus
//! configured variants). The first occurrence of each entity contributes one
//! Mention; we track the character offset within the source text and the
//! 1-based ordinal rank among all matches in the run.
//!
//! Ranking per PRD §6.1 / FR-5:
//! - If the Brand has no mention, ranking is `None`.
//! - Otherwise the Brand's ranking is the 1-based position of its first
//!   Mention among all Brand + Competitor Mentions sorted by `char_offset`.

use serde::{Deserialize, Serialize};

use opengeo_core::{BrandConfig, CompetitorConfig, Config};

use crate::sentiment::{classify_sentiment, Sentiment};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mention {
    /// Canonical entity name. For the brand: `BrandConfig::name`. For each
    /// competitor: the `CompetitorConfig::name`.
    pub entity: String,
    /// 0-based character offset of the matched substring.
    pub char_offset: usize,
    /// 1-based ordinal among all mentions in the run after sorting by
    /// `char_offset`.
    pub rank: usize,
    /// The substring as it appeared in the response.
    pub matched_text: String,
    /// True if this mention is the brand (vs a competitor).
    pub is_brand: bool,
    /// Deterministic mention-level brand-tone signal.
    pub sentiment: Sentiment,
}

/// Extract all mentions for the configured brand + competitors, sorted by
/// `char_offset`. Each entity contributes at most one Mention (its earliest
/// match) — this matches the PRD's "ordinal rank" semantics where the
/// Brand's position relative to competitors is what matters, not the count.
pub fn extract_mentions(message: &str, config: &Config) -> Vec<Mention> {
    let mut hits: Vec<Mention> = Vec::new();

    // Brand sweep.
    if let Some((offset, matched)) =
        first_match(message, &config.brand.name, &config.brand.variants)
    {
        hits.push(Mention {
            entity: config.brand.name.clone(),
            char_offset: offset,
            rank: 0, // filled below
            matched_text: matched,
            is_brand: true,
            sentiment: classify_sentiment(message, offset),
        });
    }

    // Competitor sweep.
    for comp in &config.competitors {
        if let Some((offset, matched)) = first_match(message, &comp.name, &comp.variants) {
            hits.push(Mention {
                entity: comp.name.clone(),
                char_offset: offset,
                rank: 0,
                matched_text: matched,
                is_brand: false,
                sentiment: classify_sentiment(message, offset),
            });
        }
    }

    // Sort by appearance order, assign 1-based rank.
    hits.sort_by_key(|m| m.char_offset);
    for (idx, m) in hits.iter_mut().enumerate() {
        m.rank = idx + 1;
    }
    hits
}

/// Compute the Brand's ranking per FR-5. `None` when the brand is absent.
pub fn compute_ranking(mentions: &[Mention]) -> Option<usize> {
    mentions.iter().find(|m| m.is_brand).map(|m| m.rank)
}

/// Find the earliest case-insensitive match of `canonical` OR any `variant`
/// in `haystack`. Returns `(offset_in_haystack, matched_substring)`.
fn first_match(haystack: &str, canonical: &str, variants: &[String]) -> Option<(usize, String)> {
    // Build a length-descending list so we prefer the longest possible match
    // at the earliest position — e.g. "Acme Inc." over "Acme" when both are
    // declared as variants of the same brand.
    let mut candidates: Vec<&str> = std::iter::once(canonical)
        .chain(variants.iter().map(String::as_str))
        .filter(|s| !s.is_empty())
        .collect();
    candidates.sort_by_key(|s| std::cmp::Reverse(s.len()));

    let haystack_lower = haystack.to_lowercase();
    let mut best: Option<(usize, &str)> = None;
    for needle in candidates {
        let needle_lower = needle.to_lowercase();
        if let Some(pos) = haystack_lower.find(&needle_lower) {
            match best {
                // Earlier pos wins. On tie, the longer needle wins because we
                // iterate length-descending and only overwrite on a strict
                // improvement in position.
                Some((cur, _)) if cur < pos => continue,
                Some((cur, prev)) if cur == pos && prev.len() >= needle.len() => continue,
                _ => best = Some((pos, needle)),
            }
        }
    }
    best.map(|(pos, needle)| {
        let end = pos + needle.len();
        let matched = haystack
            .get(pos..end)
            .map(str::to_string)
            .unwrap_or_else(|| needle.to_string());
        (pos, matched)
    })
}

/// Convenience builder used by tests.
pub fn config_with(brand: &str, competitors: &[&str]) -> Config {
    Config {
        schema_version: "0.1".into(),
        brand: BrandConfig {
            name: brand.into(),
            variants: vec![],
            site_url: None,
        },
        competitors: competitors
            .iter()
            .map(|c| CompetitorConfig {
                name: (*c).into(),
                variants: vec![],
            })
            .collect(),
        prompts: vec![],
        providers: vec![],
        schedules: vec![],
        concurrency: 4,
        anomaly_sensitivity: Default::default(),
        analytics: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_first_ranks_one() {
        let cfg = config_with("Acme", &["Beta Corp"]);
        let msg = "Acme is the best, followed by Beta Corp.";
        let mentions = extract_mentions(msg, &cfg);
        assert_eq!(mentions.len(), 2);
        assert_eq!(compute_ranking(&mentions), Some(1));
        assert!(mentions[0].is_brand);
        assert!(!mentions[1].is_brand);
        assert_eq!(mentions[0].sentiment.label.as_str(), "positive");
    }

    #[test]
    fn brand_after_two_competitors_ranks_three() {
        let cfg = config_with("Acme", &["Beta Corp", "Gamma Labs"]);
        let msg = "Beta Corp leads, then Gamma Labs, with Acme close behind.";
        let mentions = extract_mentions(msg, &cfg);
        assert_eq!(mentions.len(), 3);
        assert_eq!(compute_ranking(&mentions), Some(3));
    }

    #[test]
    fn brand_absent_ranks_none() {
        let cfg = config_with("Acme", &["Beta Corp"]);
        let msg = "Beta Corp is the only thing here.";
        let mentions = extract_mentions(msg, &cfg);
        assert_eq!(compute_ranking(&mentions), None);
        assert_eq!(mentions.len(), 1);
        assert!(!mentions[0].is_brand);
    }

    #[test]
    fn variants_match_case_insensitively() {
        let mut cfg = config_with("Acme", &[]);
        cfg.brand.variants = vec!["acme inc.".into()];
        let msg = "What's the deal with ACME INC.?";
        let mentions = extract_mentions(msg, &cfg);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].matched_text, "ACME INC.");
    }

    #[test]
    fn first_match_wins_for_an_entity() {
        let cfg = config_with("Acme", &[]);
        let msg = "Acme is mentioned first, then Acme again later.";
        let mentions = extract_mentions(msg, &cfg);
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].char_offset, 0);
    }

    #[test]
    fn mention_carries_byte_stable_sentiment() {
        let cfg = config_with("Acme", &[]);
        let msg = "Acme is slow and unreliable compared with alternatives.";
        let first = extract_mentions(msg, &cfg);
        let second = extract_mentions(msg, &cfg);
        assert_eq!(first, second);
        assert_eq!(first[0].sentiment.label.as_str(), "negative");
        assert!((0..=100).contains(&first[0].sentiment.score));
        assert_eq!(first[0].sentiment.lane, "deterministic_lexicon");
    }
}
