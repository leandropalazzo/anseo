//! MinHash-based prompt similarity index (Story 0.10).
//!
//! Pure-Rust, deterministic, dependency-free (`std` + already-vendored
//! `sha2`/`serde` only) Jaccard-similarity index over the configured
//! prompts. The browser extension hits `/v1/prompts/similarity-index` to
//! find which of the project's configured prompts are close to the prompt
//! the user is typing into a chat surface; the response gates whether the
//! extension shows rank data inline.
//!
//! # Algorithm
//!
//! 1. **Shingle** each prompt into the set of consecutive 3-grams over
//!    *word tokens* (after lowercasing + ASCII-word split). Word-level
//!    n-grams give better topical similarity than char-level for the short,
//!    English-ish prompts we expect; 3 is the smallest n that still
//!    discriminates ("how to do X" vs "how to do Y" share the prefix).
//! 2. **MinHash signature**: for each of [`NUM_HASH_FUNCTIONS`] = 128
//!    seeded hash functions, take the minimum hash over the shingle set.
//!    Two signatures' element-wise equality rate is an unbiased estimator
//!    of Jaccard similarity (classical MinHash result, Broder 1997).
//! 3. **Query**: hash the input the same way, then for each indexed prompt
//!    count matching signature slots ÷ 128.
//!
//! # Determinism
//!
//! All hashing uses [`std::hash::SipHasher13`] via [`std::hash::BuildHasher`]
//! re-seeded per slot from a fixed seed table. Same input bytes always
//! produce the same signature; the index is stable across processes and
//! Rust versions (SipHash-1-3 is part of std's stable hashing surface for
//! `DefaultHasher`).
//!
//! # Cost
//!
//! Build is O(prompts × shingles × 128); for the realistic Phase 3 ceiling
//! (≤ a few hundred configured prompts, each ≤ a few hundred tokens) the
//! whole index fits in well under a millisecond and a few hundred KB. The
//! API handler can rebuild per request without a measurable hit; an
//! `AppState` cache is therefore optional and we currently rebuild per
//! request (see `apps/api/src/routes/prompts_similarity.rs`).

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use serde::Serialize;
use sha2::{Digest, Sha256};

/// Number of MinHash functions. 128 gives a standard-deviation of
/// ~0.044 on the Jaccard estimate (sqrt(J(1-J)/k) at J=0.5) which is
/// comfortable for a 0.6 threshold while keeping signatures tiny
/// (128 × u64 = 1 KiB per prompt).
pub const NUM_HASH_FUNCTIONS: usize = 128;

/// Shingle size in word tokens. See module docs.
pub const SHINGLE_SIZE: usize = 3;

/// Fixed seed table used to derive the 128 hash functions. The constants
/// are arbitrary but FROZEN — changing them changes every signature and
/// therefore is a breaking change for any persisted index or cached
/// response.
const SEED_BASE: u64 = 0x9E37_79B9_7F4A_7C15; // golden-ratio mix (Knuth)

/// One indexed prompt + its MinHash signature.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    text: String,
    signature: [u64; NUM_HASH_FUNCTIONS],
}

/// In-memory MinHash index over the configured prompts.
#[derive(Debug, Clone, Default)]
pub struct MinHashIndex {
    entries: Vec<Entry>,
}

/// One row in [`MinHashIndex::query`]'s result.
#[derive(Debug, Clone, Serialize)]
pub struct Match {
    /// Prompt name (slug from `anseo.yaml`).
    pub name: String,
    /// Prompt text.
    pub prompt: String,
    /// MinHash-estimated Jaccard, in `[0.0, 1.0]`.
    pub estimated_jaccard: f32,
}

impl MinHashIndex {
    /// Build the index from `(name, text)` pairs. Empty texts yield an
    /// all-`u64::MAX` signature and will never match above 0.
    pub fn build(prompts: &[(String, String)]) -> Self {
        let entries = prompts
            .iter()
            .map(|(name, text)| Entry {
                name: name.clone(),
                text: text.clone(),
                signature: minhash_signature(text),
            })
            .collect();
        Self { entries }
    }

    /// Query for prompts above `threshold` Jaccard, returning up to
    /// `limit` matches sorted by estimated Jaccard descending. Ties broken
    /// by prompt name ascending for determinism.
    pub fn query(&self, text: &str, threshold: f32, limit: usize) -> Vec<Match> {
        let q = minhash_signature(text);
        let mut scored: Vec<Match> = self
            .entries
            .iter()
            .map(|e| {
                let matches = q
                    .iter()
                    .zip(e.signature.iter())
                    .filter(|(a, b)| a == b)
                    .count();
                let jaccard = matches as f32 / NUM_HASH_FUNCTIONS as f32;
                Match {
                    name: e.name.clone(),
                    prompt: e.text.clone(),
                    estimated_jaccard: jaccard,
                }
            })
            .filter(|m| m.estimated_jaccard >= threshold)
            .collect();
        scored.sort_by(|a, b| {
            b.estimated_jaccard
                .partial_cmp(&a.estimated_jaccard)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.name.cmp(&b.name))
        });
        scored.truncate(limit);
        scored
    }

    /// Number of indexed prompts.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Is the index empty?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// SHA-256 of the input text, hex-encoded with a `sha256:` prefix. Used
/// in the API response so the caller can dedupe identical queries
/// client-side.
pub fn sha256_input(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// Word-tokenize: lowercase, then split on any non-alphanumeric ASCII run.
/// Unicode letters are dropped — Phase 3 only targets English chat input;
/// extending to Unicode word-segmentation is a Phase 4 concern.
fn tokenize(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Produce the set of 3-gram shingles over the token stream. If the text
/// has fewer than 3 tokens we fall back to a single shingle of the whole
/// token stream so short prompts can still match each other.
fn shingles(text: &str) -> Vec<String> {
    let tokens = tokenize(text);
    if tokens.is_empty() {
        return vec![];
    }
    if tokens.len() < SHINGLE_SIZE {
        return vec![tokens.join(" ")];
    }
    tokens.windows(SHINGLE_SIZE).map(|w| w.join(" ")).collect()
}

/// Hash one shingle with the `slot`-th seeded hasher.
fn hash_with_seed(shingle: &str, slot: usize) -> u64 {
    // Derive a slot-specific seed via a deterministic mix of the base seed
    // and the slot index. We feed that seed in as a prefix to SipHash;
    // std's `BuildHasherDefault<SipHasher13>` doesn't expose a seeded
    // constructor, so prefixing is the portable way to get k independent
    // hash functions out of one stable hash.
    let seed = SEED_BASE
        .wrapping_mul(slot as u64 + 1)
        .wrapping_add(slot as u64);
    let mut hasher = DefaultHasher::new();
    hasher.write_u64(seed);
    hasher.write(shingle.as_bytes());
    hasher.finish()
}

fn minhash_signature(text: &str) -> [u64; NUM_HASH_FUNCTIONS] {
    let mut sig = [u64::MAX; NUM_HASH_FUNCTIONS];
    let shingles = shingles(text);
    if shingles.is_empty() {
        return sig;
    }
    for shingle in &shingles {
        for (slot, slot_value) in sig.iter_mut().enumerate() {
            let h = hash_with_seed(shingle, slot);
            if h < *slot_value {
                *slot_value = h;
            }
        }
    }
    sig
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, text: &str) -> (String, String) {
        (name.to_string(), text.to_string())
    }

    #[test]
    fn exact_match_returns_perfect_jaccard() {
        let prompts = vec![p("a", "best crm for small business")];
        let idx = MinHashIndex::build(&prompts);
        let m = idx.query("best crm for small business", 0.5, 10);
        assert_eq!(m.len(), 1);
        assert!(
            (m[0].estimated_jaccard - 1.0).abs() < f32::EPSILON,
            "expected 1.0, got {}",
            m[0].estimated_jaccard
        );
    }

    #[test]
    fn near_match_passes_default_threshold() {
        // Same 3-gram backbone except one swapped trailing token.
        let prompts = vec![p("a", "best crm for small business owners today")];
        let idx = MinHashIndex::build(&prompts);
        let m = idx.query("best crm for small business owners now", 0.5, 10);
        assert_eq!(m.len(), 1, "near match should land above 0.5");
        assert!(m[0].estimated_jaccard >= 0.5);
        assert!(m[0].estimated_jaccard < 1.0);
    }

    #[test]
    fn far_match_returns_nothing() {
        let prompts = vec![p("a", "best crm for small business")];
        let idx = MinHashIndex::build(&prompts);
        let m = idx.query("how to bake a sourdough loaf at home in winter", 0.6, 10);
        assert!(
            m.is_empty(),
            "unrelated prompt should not match, got {:?}",
            m
        );
    }

    #[test]
    fn deterministic_across_builds() {
        let prompts = vec![
            p("a", "best crm for small business"),
            p("b", "top project management tools"),
        ];
        let i1 = MinHashIndex::build(&prompts);
        let i2 = MinHashIndex::build(&prompts);
        let q1 = i1.query("best crm tools", 0.0, 10);
        let q2 = i2.query("best crm tools", 0.0, 10);
        assert_eq!(q1.len(), q2.len());
        for (a, b) in q1.iter().zip(q2.iter()) {
            assert_eq!(a.name, b.name);
            assert!((a.estimated_jaccard - b.estimated_jaccard).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn ranking_is_stable_and_sorted_desc() {
        let prompts = vec![
            p("close", "best crm for small business"),
            p("far", "how to bake sourdough bread"),
            p("middle", "best crm features for teams"),
        ];
        let idx = MinHashIndex::build(&prompts);
        let m = idx.query("best crm for small business", 0.0, 10);
        // Sorted descending.
        for w in m.windows(2) {
            assert!(w[0].estimated_jaccard >= w[1].estimated_jaccard);
        }
        // Exact-match prompt is first.
        assert_eq!(m[0].name, "close");
    }

    #[test]
    fn limit_caps_result_count() {
        let prompts: Vec<_> = (0..20)
            .map(|i| p(&format!("p{i:02}"), "best crm for small business"))
            .collect();
        let idx = MinHashIndex::build(&prompts);
        let m = idx.query("best crm for small business", 0.0, 5);
        assert_eq!(m.len(), 5);
    }

    #[test]
    fn empty_input_yields_no_matches() {
        let prompts = vec![p("a", "best crm for small business")];
        let idx = MinHashIndex::build(&prompts);
        assert!(idx.query("", 0.1, 10).is_empty());
    }

    #[test]
    fn sha256_input_is_stable() {
        assert_eq!(
            sha256_input("hello"),
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn short_prompts_below_shingle_size_still_match() {
        let prompts = vec![p("a", "best crm")];
        let idx = MinHashIndex::build(&prompts);
        let m = idx.query("best crm", 0.5, 10);
        assert_eq!(m.len(), 1);
        assert!((m[0].estimated_jaccard - 1.0).abs() < f32::EPSILON);
    }
}
