//! Deterministic mention-level sentiment classification.
//!
//! This is intentionally small and lexicon-backed. It gives the deterministic
//! lane a byte-stable result, and keeps richer/non-deterministic classifiers as
//! an additive future lane instead of making extraction depend on an LLM.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentimentLabel {
    Positive,
    Neutral,
    Negative,
}

impl SentimentLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            SentimentLabel::Positive => "positive",
            SentimentLabel::Neutral => "neutral",
            SentimentLabel::Negative => "negative",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sentiment {
    pub label: SentimentLabel,
    /// Deterministic bounded score. Higher means more positive; neutral is 50.
    pub score: u8,
    /// Reproducibility lane tag. This classifier is byte-stable for fixed input.
    pub lane: String,
}

impl Sentiment {
    pub fn neutral() -> Self {
        Self {
            label: SentimentLabel::Neutral,
            score: 50,
            lane: "deterministic_lexicon".into(),
        }
    }
}

const POSITIVE_TERMS: &[&str] = &[
    "accurate",
    "best",
    "fast",
    "good",
    "great",
    "leading",
    "recommended",
    "reliable",
    "robust",
    "strong",
];

const NEGATIVE_TERMS: &[&str] = &[
    "bad",
    "buggy",
    "expensive",
    "fails",
    "limited",
    "poor",
    "slow",
    "unreliable",
    "weak",
    "worse",
];

/// Classify a mention using a bounded context window around its character
/// offset. The function cannot fail; extraction callers can always persist a
/// mention even if future classifier lanes become optional/degradable.
pub fn classify_sentiment(message: &str, char_offset: usize) -> Sentiment {
    let context = context_window(message, char_offset, 160).to_lowercase();
    let positive = count_terms(&context, POSITIVE_TERMS);
    let negative = count_terms(&context, NEGATIVE_TERMS);

    match positive.cmp(&negative) {
        std::cmp::Ordering::Greater => Sentiment {
            label: SentimentLabel::Positive,
            score: (60 + (positive - negative).min(4) * 10) as u8,
            lane: "deterministic_lexicon".into(),
        },
        std::cmp::Ordering::Less => Sentiment {
            label: SentimentLabel::Negative,
            score: (40i32 - ((negative - positive).min(4) as i32 * 10)).max(0) as u8,
            lane: "deterministic_lexicon".into(),
        },
        std::cmp::Ordering::Equal => Sentiment::neutral(),
    }
}

fn context_window(message: &str, char_offset: usize, radius: usize) -> &str {
    let indices: Vec<usize> = message.char_indices().map(|(idx, _)| idx).collect();
    let mention_char_idx = indices
        .iter()
        .position(|idx| *idx >= char_offset)
        .unwrap_or(indices.len().saturating_sub(1));
    let start = indices
        .get(mention_char_idx.saturating_sub(radius))
        .copied()
        .unwrap_or(0);
    let end = indices
        .get((mention_char_idx + radius).min(indices.len()))
        .copied()
        .unwrap_or(message.len());
    &message[start..end]
}

fn count_terms(context: &str, terms: &[&str]) -> usize {
    context
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| terms.iter().any(|term| token == term))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_negative_and_neutral_are_deterministic() {
        let positive = classify_sentiment("Acme is a fast, reliable, recommended tool.", 0);
        assert_eq!(positive.label, SentimentLabel::Positive);
        assert_eq!(positive.score, 90);
        assert_eq!(positive.lane, "deterministic_lexicon");

        let negative = classify_sentiment("Acme is slow and unreliable.", 0);
        assert_eq!(negative.label, SentimentLabel::Negative);
        assert_eq!(negative.score, 20);

        let neutral = classify_sentiment("Acme is a vector database.", 0);
        assert_eq!(neutral, Sentiment::neutral());
    }
}
