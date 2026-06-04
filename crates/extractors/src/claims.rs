//! Deterministic claim extraction for the Epic 34 OSS data layer.
//!
//! The premium hallucination engine judges claims later. This extractor only
//! stores factual-looking brand statements as reviewable data.

use serde::{Deserialize, Serialize};

use opengeo_core::Config;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub entity: String,
    pub claim_text: String,
    pub claim_kind: String,
    pub char_offset: Option<usize>,
    pub confidence: u8,
    pub extractor_lane: String,
}

/// Extract one claim per sentence-like span that mentions the tracked brand or
/// one of its variants. The first implementation is intentionally conservative:
/// no LLM judgment, no truth scoring, no commercial dependency.
pub fn extract_claims(message: &str, config: &Config) -> Vec<Claim> {
    sentence_spans(message)
        .into_iter()
        .filter_map(|(offset, sentence)| {
            if mentions_brand(sentence, config) {
                Some(Claim {
                    entity: config.brand.name.clone(),
                    claim_text: sentence.trim().to_string(),
                    claim_kind: "factual_statement".to_string(),
                    char_offset: Some(offset),
                    confidence: 80,
                    extractor_lane: "deterministic_sentence".to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn mentions_brand(sentence: &str, config: &Config) -> bool {
    let lower = sentence.to_lowercase();
    std::iter::once(config.brand.name.as_str())
        .chain(config.brand.variants.iter().map(String::as_str))
        .filter(|candidate| !candidate.trim().is_empty())
        .any(|candidate| lower.contains(&candidate.to_lowercase()))
}

fn sentence_spans(message: &str) -> Vec<(usize, &str)> {
    let mut spans = Vec::new();
    let mut start = 0usize;

    for (idx, ch) in message.char_indices() {
        if matches!(ch, '.' | '!' | '?' | '\n') {
            push_nonempty_span(message, start, idx + ch.len_utf8(), &mut spans);
            start = idx + ch.len_utf8();
        }
    }
    if start < message.len() {
        push_nonempty_span(message, start, message.len(), &mut spans);
    }

    spans
}

fn push_nonempty_span<'a>(
    message: &'a str,
    start: usize,
    end: usize,
    spans: &mut Vec<(usize, &'a str)>,
) {
    let span = &message[start..end];
    let leading = span.len() - span.trim_start().len();
    let trimmed = span.trim();
    if !trimmed.is_empty() {
        spans.push((start + leading, trimmed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mentions::config_with;

    #[test]
    fn extracts_brand_sentence_claims() {
        let cfg = config_with("Acme", &["Beta Corp"]);
        let text = "Beta Corp is mentioned first. Acme is SOC2 certified. Acme supports SSO.";

        let claims = extract_claims(text, &cfg);

        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].entity, "Acme");
        assert_eq!(claims[0].claim_text, "Acme is SOC2 certified.");
        assert_eq!(claims[0].claim_kind, "factual_statement");
        assert_eq!(claims[0].extractor_lane, "deterministic_sentence");
        assert_eq!(claims[0].char_offset, Some(30));
    }

    #[test]
    fn brand_variants_count_as_brand_claims() {
        let mut cfg = config_with("Acme", &[]);
        cfg.brand.variants = vec!["Acme Inc".to_string()];

        let claims = extract_claims("Acme Inc has offices in Amsterdam.", &cfg);

        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].entity, "Acme");
    }

    #[test]
    fn no_brand_no_claims() {
        let cfg = config_with("Acme", &[]);

        assert!(extract_claims("Beta Corp is headquartered in Paris.", &cfg).is_empty());
    }
}
