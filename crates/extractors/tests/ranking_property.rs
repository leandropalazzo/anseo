//! Ranking property test (P0-005, FR-5).
//!
//! Property-based assertions about `compute_ranking(extract_mentions(...))`
//! that hold for any synthesized text, independent of the specific fixtures
//! we have on disk:
//!
//! 1. Brand absent ⇒ `compute_ranking` is `None`.
//! 2. Brand present and is the leftmost mention ⇒ rank = 1.
//! 3. Brand present and is the rightmost mention ⇒ rank = mentions.len().
//! 4. Rank is always in the closed interval `[1, mentions.len()]` when
//!    `Some(_)`.
//!
//! Generators are intentionally narrow: ASCII-only entity tokens separated by
//! whitespace and punctuation. The point of this test is to pin the *ranking
//! invariants*, not to fuzz the matching regex (the fixture-set snapshots
//! already exercise broad text shapes).
//!
//! trace: P0-005 (FR-5 ranking semantics)

use opengeo_extractors::{compute_ranking, extract_mentions, mentions::config_with};
use proptest::prelude::*;

/// One of: brand or a competitor. The string is the chosen entity name.
#[derive(Debug, Clone)]
enum Token {
    Brand,
    Competitor(usize), // index into the competitor list
}

const BRAND: &str = "Pinecone";
const COMPETITORS: [&str; 3] = ["Qdrant", "Weaviate", "Chroma"];

fn token_to_text(tok: &Token) -> &'static str {
    match tok {
        Token::Brand => BRAND,
        Token::Competitor(i) => COMPETITORS[i % COMPETITORS.len()],
    }
}

/// Render a token sequence into prose. Each token becomes one sentence so
/// matches are unambiguous.
fn render(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|t| format!("{} is a vector database.", token_to_text(t)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Tokens with at most one Brand, since `extract_mentions` only records the
/// first occurrence of each entity — duplicates would silently collapse and
/// confuse the rank assertions.
fn arb_token_sequence() -> impl Strategy<Value = Vec<Token>> {
    // Build a sequence of 1..=8 entity slots. To keep at most one Brand, we
    // first generate competitor-only tokens and then optionally insert a
    // single Brand at a random position.
    let competitors_only = prop::collection::vec(
        (0usize..COMPETITORS.len()).prop_map(Token::Competitor),
        0..=7,
    );
    (competitors_only, prop::option::of(any::<u32>())).prop_map(|(mut tokens, insert)| {
        if let Some(seed) = insert {
            let pos = (seed as usize) % (tokens.len() + 1);
            tokens.insert(pos, Token::Brand);
        }
        tokens
    })
}

fn config() -> opengeo_core::Config {
    config_with(BRAND, &COMPETITORS)
}

proptest! {
    /// Brand-absent ⇒ ranking is None.
    #[test]
    fn brand_absent_ranks_none(
        // Only competitor tokens — no Brand inserted.
        comps in prop::collection::vec((0usize..COMPETITORS.len()).prop_map(Token::Competitor), 0..=6)
    ) {
        let body = render(&comps);
        let mentions = extract_mentions(&body, &config());
        prop_assert!(mentions.iter().all(|m| !m.is_brand));
        prop_assert_eq!(compute_ranking(&mentions), None);
    }

    /// Brand first in text ⇒ rank == 1.
    #[test]
    fn brand_first_ranks_one(
        tail in prop::collection::vec((0usize..COMPETITORS.len()).prop_map(Token::Competitor), 0..=6)
    ) {
        let mut tokens = vec![Token::Brand];
        tokens.extend(tail);
        let body = render(&tokens);
        let mentions = extract_mentions(&body, &config());
        prop_assert_eq!(compute_ranking(&mentions), Some(1));
    }

    /// Brand last in text (after at least one competitor) ⇒ rank == mentions.len().
    #[test]
    fn brand_last_ranks_n(
        comps in prop::collection::vec((0usize..COMPETITORS.len()).prop_map(Token::Competitor), 1..=6)
    ) {
        let mut tokens = comps.clone();
        tokens.push(Token::Brand);
        let body = render(&tokens);
        let mentions = extract_mentions(&body, &config());
        prop_assert!(!mentions.is_empty());
        prop_assert_eq!(compute_ranking(&mentions), Some(mentions.len()));
    }

    /// Rank invariant: when Some(r), 1 <= r <= mentions.len(); when None,
    /// no mention has is_brand=true.
    #[test]
    fn rank_is_in_bounds(tokens in arb_token_sequence()) {
        let body = render(&tokens);
        let mentions = extract_mentions(&body, &config());
        match compute_ranking(&mentions) {
            Some(r) => {
                prop_assert!(r >= 1, "rank must be 1-based, got {r}");
                prop_assert!(r <= mentions.len(), "rank {r} exceeds mentions.len() = {}", mentions.len());
                prop_assert!(mentions.iter().any(|m| m.is_brand), "Some(_) rank implies a brand mention exists");
            }
            None => {
                prop_assert!(!mentions.iter().any(|m| m.is_brand), "None rank implies no brand mention");
            }
        }
    }
}
