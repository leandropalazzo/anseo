//! Run-to-run diff (FR-6a).
//!
//! Compares two `(Mention[], Citation[])` snapshots and reports added /
//! dropped entries. The label "auto-extracted summary" is the dashboard's
//! responsibility — this module just produces the structured diff.

use serde::{Deserialize, Serialize};

use crate::citations::Citation;
use crate::mentions::Mention;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunExtractionDiff {
    pub added_mentions: Vec<Mention>,
    pub dropped_mentions: Vec<Mention>,
    pub added_citations: Vec<Citation>,
    pub dropped_citations: Vec<Citation>,
}

pub fn diff_extractions(
    prior_mentions: &[Mention],
    current_mentions: &[Mention],
    prior_citations: &[Citation],
    current_citations: &[Citation],
) -> RunExtractionDiff {
    let added_mentions = current_mentions
        .iter()
        .filter(|m| !prior_mentions.iter().any(|p| same_entity(p, m)))
        .cloned()
        .collect();
    let dropped_mentions = prior_mentions
        .iter()
        .filter(|m| !current_mentions.iter().any(|c| same_entity(c, m)))
        .cloned()
        .collect();
    let added_citations = current_citations
        .iter()
        .filter(|c| !prior_citations.iter().any(|p| same_citation(p, c)))
        .cloned()
        .collect();
    let dropped_citations = prior_citations
        .iter()
        .filter(|c| !current_citations.iter().any(|n| same_citation(n, c)))
        .cloned()
        .collect();
    RunExtractionDiff {
        added_mentions,
        dropped_mentions,
        added_citations,
        dropped_citations,
    }
}

fn same_entity(a: &Mention, b: &Mention) -> bool {
    a.entity.eq_ignore_ascii_case(&b.entity) && a.is_brand == b.is_brand
}

fn same_citation(a: &Citation, b: &Citation) -> bool {
    a.domain == b.domain && a.url == b.url
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::citations::SourceType;
    use crate::sentiment::Sentiment;

    fn mention(entity: &str, is_brand: bool) -> Mention {
        Mention {
            entity: entity.into(),
            char_offset: 0,
            rank: 1,
            matched_text: entity.into(),
            is_brand,
            sentiment: Sentiment::neutral(),
        }
    }

    fn citation(domain: &str, url: Option<&str>) -> Citation {
        Citation {
            url: url.map(str::to_string),
            domain: domain.into(),
            frequency: 1,
            source_type: Some(SourceType::GeneralWeb),
        }
    }

    #[test]
    fn detects_added_and_dropped_mentions() {
        let prior = vec![mention("Acme", true), mention("Beta", false)];
        let current = vec![mention("Acme", true), mention("Gamma", false)];
        let diff = diff_extractions(&prior, &current, &[], &[]);
        assert_eq!(diff.added_mentions.len(), 1);
        assert_eq!(diff.added_mentions[0].entity, "Gamma");
        assert_eq!(diff.dropped_mentions.len(), 1);
        assert_eq!(diff.dropped_mentions[0].entity, "Beta");
    }

    #[test]
    fn detects_added_and_dropped_citations() {
        let prior = vec![citation("example.com", Some("https://example.com/a"))];
        let current = vec![citation("example.com", Some("https://example.com/b"))];
        let diff = diff_extractions(&[], &[], &prior, &current);
        assert_eq!(diff.added_citations.len(), 1);
        assert_eq!(diff.dropped_citations.len(), 1);
    }

    #[test]
    fn empty_diff_for_identical_runs() {
        let m = vec![mention("Acme", true)];
        let c = vec![citation("example.com", None)];
        let diff = diff_extractions(&m, &m, &c, &c);
        assert!(diff.added_mentions.is_empty());
        assert!(diff.dropped_mentions.is_empty());
        assert!(diff.added_citations.is_empty());
        assert!(diff.dropped_citations.is_empty());
    }
}
