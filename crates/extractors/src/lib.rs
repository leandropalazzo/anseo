//! Mention + citation extraction for Phase 1 (FR-3, FR-4, FR-5).
//!
//! Stateless functions. Take a Provider message body and a [`Config`]; emit
//! `Vec<Mention>` and `Vec<Citation>` ready for the storage layer.

pub mod citations;
pub mod claims;
pub mod diff;
pub mod mentions;
pub mod persist;
pub mod sentiment;

pub use citations::{
    extract_citations, extract_citations_from_annotations, merge_citations, Citation, SourceType,
};
pub use claims::{extract_claims, Claim};
pub use diff::{diff_extractions, RunExtractionDiff};
pub use mentions::{compute_ranking, extract_mentions, Mention};
pub use persist::extract_and_persist;
pub use sentiment::{classify_sentiment, Sentiment, SentimentLabel};
