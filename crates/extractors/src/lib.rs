//! Mention + citation extraction for Phase 1 (FR-3, FR-4, FR-5).
//!
//! Stateless functions. Take a Provider message body and a [`Config`]; emit
//! `Vec<Mention>` and `Vec<Citation>` ready for the storage layer.

pub mod citations;
pub mod diff;
pub mod mentions;

pub use citations::{extract_citations, Citation, SourceType};
pub use diff::{diff_extractions, RunExtractionDiff};
pub use mentions::{compute_ranking, extract_mentions, Mention};
