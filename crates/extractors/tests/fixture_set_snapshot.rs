//! Canonical fixture-set snapshot test (P0-003, P0-004 — FR-3, FR-4).
//!
//! The six fixtures in `tests/fixtures/` are representative LLM response
//! shapes the Phase 1 extractor must handle:
//!
//!  1. `01-openai-typical.txt`        — narrative paragraph + one bare URL
//!  2. `02-openai-markdown-links.txt` — multiple `[label](url)` citations
//!  3. `03-openai-bare-domains.txt`   — bare-domain citations (no scheme)
//!  4. `04-anthropic-typical.txt`     — Anthropic-style narrative
//!  5. `05-anthropic-reference-list.txt` — numbered `[1]` references
//!  6. `06-multibyte-japanese.txt`    — Japanese text (R-010 multibyte)
//!
//! Each fixture is run through both `extract_mentions` and `extract_citations`
//! and the structured output is snapshot-tested with `insta`. Snapshots are
//! checked into source; any future change to extraction logic that affects
//! these shapes will fail CI until the snapshots are reviewed and accepted.
//!
//! Brand and competitor config is identical across fixtures so the snapshots
//! describe extractor behavior, not config interactions.
//!
//! trace: P0-003 (FR-3 mention extraction)
//! trace: P0-004 (FR-4 citation extraction)
//! trace: P2-003 (FR-3 multibyte UTF-8 offsets — covered by fixture 06)

use anseo_extractors::{extract_citations, extract_mentions, mentions::config_with};
use insta::assert_yaml_snapshot;

fn read_fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()))
}

fn canonical_config() -> anseo_core::Config {
    // Shared across every fixture so the snapshots describe extractor
    // behavior, not config differences.
    config_with("Pinecone", &["Qdrant", "Weaviate", "Chroma"])
}

#[test]
fn fixture_01_openai_typical_mentions() {
    let body = read_fixture("01-openai-typical.txt");
    assert_yaml_snapshot!(extract_mentions(&body, &canonical_config()));
}

#[test]
fn fixture_01_openai_typical_citations() {
    let body = read_fixture("01-openai-typical.txt");
    assert_yaml_snapshot!(extract_citations(&body));
}

#[test]
fn fixture_02_openai_markdown_links_mentions() {
    let body = read_fixture("02-openai-markdown-links.txt");
    assert_yaml_snapshot!(extract_mentions(&body, &canonical_config()));
}

#[test]
fn fixture_02_openai_markdown_links_citations() {
    let body = read_fixture("02-openai-markdown-links.txt");
    assert_yaml_snapshot!(extract_citations(&body));
}

#[test]
fn fixture_03_openai_bare_domains_mentions() {
    let body = read_fixture("03-openai-bare-domains.txt");
    assert_yaml_snapshot!(extract_mentions(&body, &canonical_config()));
}

#[test]
fn fixture_03_openai_bare_domains_citations() {
    let body = read_fixture("03-openai-bare-domains.txt");
    assert_yaml_snapshot!(extract_citations(&body));
}

#[test]
fn fixture_04_anthropic_typical_mentions() {
    let body = read_fixture("04-anthropic-typical.txt");
    assert_yaml_snapshot!(extract_mentions(&body, &canonical_config()));
}

#[test]
fn fixture_04_anthropic_typical_citations() {
    let body = read_fixture("04-anthropic-typical.txt");
    assert_yaml_snapshot!(extract_citations(&body));
}

#[test]
fn fixture_05_anthropic_reference_list_mentions() {
    let body = read_fixture("05-anthropic-reference-list.txt");
    assert_yaml_snapshot!(extract_mentions(&body, &canonical_config()));
}

#[test]
fn fixture_05_anthropic_reference_list_citations() {
    let body = read_fixture("05-anthropic-reference-list.txt");
    assert_yaml_snapshot!(extract_citations(&body));
}

#[test]
fn fixture_06_multibyte_japanese_mentions() {
    let body = read_fixture("06-multibyte-japanese.txt");
    assert_yaml_snapshot!(extract_mentions(&body, &canonical_config()));
}

#[test]
fn fixture_06_multibyte_japanese_citations() {
    let body = read_fixture("06-multibyte-japanese.txt");
    assert_yaml_snapshot!(extract_citations(&body));
}
