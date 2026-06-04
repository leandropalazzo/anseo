//! Citation extraction (FR-4) — surfaces URLs and bare domains in a Provider
//! response. Each unique `(url|domain, source_type)` collapses into one
//! Citation with a frequency count.
//!
//! Recognised surfaces (PRD §6.1 FR-4):
//! - Full URLs (`https://…`, `http://…`).
//! - Markdown links `[label](url)`.
//! - Reference-style URLs `[name]: url`.
//! - Bare domains (`example.com`, `docs.example.io`).
//!
//! Source-type inference (PRD §6.1 FR-4 "source type"):
//! - `reddit.com` → `Reddit`
//! - `*.wikipedia.org` → `Wikipedia`
//! - `youtube.com` / `youtu.be` → `Youtube`
//! - Anything matching `docs.*` or `*.dev/docs` → `Docs`
//! - Everything else → `GeneralWeb`

use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Docs,
    Reddit,
    Wikipedia,
    Youtube,
    GeneralWeb,
}

impl SourceType {
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Docs => "docs",
            Self::Reddit => "reddit",
            Self::Wikipedia => "wikipedia",
            Self::Youtube => "youtube",
            Self::GeneralWeb => "general_web",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Full URL when an absolute URL was present in the response.
    pub url: Option<String>,
    /// Always-knowable host. Always lowercased.
    pub domain: String,
    /// Number of distinct surfaces (URL or bare domain) that resolved to
    /// this `(domain, url)` pair.
    pub frequency: u32,
    pub source_type: Option<SourceType>,
}

static URL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bhttps?://[a-z0-9][a-z0-9.\-]*[a-z0-9](?::\d+)?(/[^\s)\]]*)?")
        .expect("static regex")
});

static BARE_DOMAIN_RE: Lazy<Regex> = Lazy::new(|| {
    // A reasonable bare-domain heuristic — at least two dot-separated labels,
    // last label 2–24 chars, surrounded by word boundaries.
    Regex::new(r"(?i)\b([a-z0-9](?:[a-z0-9\-]{0,61}[a-z0-9])?\.)+[a-z]{2,24}\b")
        .expect("static regex")
});

pub fn extract_citations(message: &str) -> Vec<Citation> {
    let mut by_key: HashMap<(String, Option<String>), Citation> = HashMap::new();
    let mut seen_offsets: Vec<(usize, usize)> = Vec::new();

    // 1. Absolute URLs.
    for m in URL_RE.find_iter(message) {
        let raw = m.as_str().trim_end_matches(['.', ',', ';', ':', ')']);
        let parsed = Url::parse(raw);
        if let Ok(u) = parsed {
            let domain = u.host_str().unwrap_or("").to_lowercase();
            if domain.is_empty() {
                continue;
            }
            let source_type = infer_source_type(&domain, u.path());
            let key = (domain.clone(), Some(raw.to_string()));
            let entry = by_key.entry(key).or_insert_with(|| Citation {
                url: Some(raw.to_string()),
                domain: domain.clone(),
                frequency: 0,
                source_type,
            });
            entry.frequency += 1;
            seen_offsets.push((m.start(), m.end()));
        }
    }

    // 2. Bare domains (skip ranges already absorbed by absolute URLs).
    for m in BARE_DOMAIN_RE.find_iter(message) {
        if seen_offsets
            .iter()
            .any(|(s, e)| m.start() >= *s && m.end() <= *e)
        {
            continue;
        }
        let token = m.as_str();
        // Avoid common false positives like "e.g." or fragments without a TLD.
        if token.matches('.').count() == 0 {
            continue;
        }
        let domain = token.to_lowercase();
        let key = (domain.clone(), None);
        let entry = by_key.entry(key).or_insert_with(|| Citation {
            url: None,
            domain: domain.clone(),
            frequency: 0,
            source_type: infer_source_type(&domain, "/"),
        });
        entry.frequency += 1;
    }

    // Stable order: by domain, then url presence (URL-bearing first).
    let mut out: Vec<Citation> = by_key.into_values().collect();
    out.sort_by(|a, b| a.domain.cmp(&b.domain).then(b.url.cmp(&a.url)));
    out
}

/// Extract citations from a provider response's structured citation
/// annotations. Perplexity (and OpenRouter when proxying it) attach sources
/// as `choices[].message.annotations[]` objects of shape
/// `{ "type": "url_citation", "url_citation": { "url": "…", … } }` rather than
/// inlining the URLs in the message text, so the text-scanning
/// [`extract_citations`] never sees them. Each unique `(url, domain)` collapses
/// into one citation; the count of annotation occurrences is the frequency.
pub fn extract_citations_from_annotations(raw_response: &serde_json::Value) -> Vec<Citation> {
    let mut by_key: HashMap<(String, Option<String>), Citation> = HashMap::new();

    let Some(choices) = raw_response.get("choices").and_then(|c| c.as_array()) else {
        return Vec::new();
    };
    for choice in choices {
        let Some(annotations) = choice
            .get("message")
            .and_then(|m| m.get("annotations"))
            .and_then(|a| a.as_array())
        else {
            continue;
        };
        for ann in annotations {
            let Some(url_str) = ann
                .get("url_citation")
                .and_then(|u| u.get("url"))
                .and_then(|u| u.as_str())
            else {
                continue;
            };
            let raw = url_str.trim_end_matches(['.', ',', ';', ':', ')']);
            let Ok(parsed) = Url::parse(raw) else {
                continue;
            };
            let domain = parsed.host_str().unwrap_or("").to_lowercase();
            if domain.is_empty() {
                continue;
            }
            let source_type = infer_source_type(&domain, parsed.path());
            let key = (domain.clone(), Some(raw.to_string()));
            let entry = by_key.entry(key).or_insert_with(|| Citation {
                url: Some(raw.to_string()),
                domain: domain.clone(),
                frequency: 0,
                source_type,
            });
            entry.frequency += 1;
        }
    }

    let mut out: Vec<Citation> = by_key.into_values().collect();
    out.sort_by(|a, b| a.domain.cmp(&b.domain).then(b.url.cmp(&a.url)));
    out
}

/// Merge two citation lists, summing frequencies for matching `(url, domain)`
/// pairs. Used to combine inline-text citations with structured-annotation
/// ones from the same response.
pub fn merge_citations(a: Vec<Citation>, b: Vec<Citation>) -> Vec<Citation> {
    let mut by_key: HashMap<(String, Option<String>), Citation> = HashMap::new();
    for c in a.into_iter().chain(b) {
        let key = (c.domain.clone(), c.url.clone());
        by_key
            .entry(key)
            .and_modify(|e| e.frequency += c.frequency)
            .or_insert(c);
    }
    let mut out: Vec<Citation> = by_key.into_values().collect();
    out.sort_by(|a, b| a.domain.cmp(&b.domain).then(b.url.cmp(&a.url)));
    out
}

fn infer_source_type(domain: &str, path: &str) -> Option<SourceType> {
    let d = domain.to_lowercase();
    if d.contains("reddit.com") {
        return Some(SourceType::Reddit);
    }
    if d.ends_with("wikipedia.org") {
        return Some(SourceType::Wikipedia);
    }
    if d == "youtube.com" || d.ends_with(".youtube.com") || d == "youtu.be" {
        return Some(SourceType::Youtube);
    }
    if d.starts_with("docs.") || path.starts_with("/docs") || d.ends_with(".readthedocs.io") {
        return Some(SourceType::Docs);
    }
    Some(SourceType::GeneralWeb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_absolute_url() {
        let text = "See https://docs.example.com/getting-started for details.";
        let cites = extract_citations(text);
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0].domain, "docs.example.com");
        assert_eq!(cites[0].source_type, Some(SourceType::Docs));
        assert_eq!(cites[0].frequency, 1);
    }

    #[test]
    fn infers_reddit_wikipedia_youtube() {
        let text = "Discussion at https://www.reddit.com/r/rust/comments/abc \
                    and history at https://en.wikipedia.org/wiki/Rust_(programming_language) \
                    plus the talk https://www.youtube.com/watch?v=xyz";
        let cites = extract_citations(text);
        let types: Vec<_> = cites.iter().filter_map(|c| c.source_type).collect();
        assert!(types.contains(&SourceType::Reddit));
        assert!(types.contains(&SourceType::Wikipedia));
        assert!(types.contains(&SourceType::Youtube));
    }

    #[test]
    fn collapses_duplicates_with_frequency() {
        let text = "Read https://docs.example.com/a and again https://docs.example.com/a please.";
        let cites = extract_citations(text);
        assert_eq!(cites.len(), 1);
        assert_eq!(cites[0].frequency, 2);
    }

    #[test]
    fn bare_domain_picked_up_without_overlap() {
        let text =
            "qdrant.tech is the Rust-native vector DB; see https://qdrant.tech/docs as well.";
        let cites = extract_citations(text);
        // We expect at least two distinct citations: bare + absolute. They
        // share the same domain but differ on url+path.
        assert!(cites.len() >= 2);
        let has_bare = cites
            .iter()
            .any(|c| c.url.is_none() && c.domain == "qdrant.tech");
        let has_url = cites
            .iter()
            .any(|c| c.url.as_deref() == Some("https://qdrant.tech/docs"));
        assert!(has_bare && has_url);
    }

    #[test]
    fn snake_case_source_type_wire_strings() {
        assert_eq!(SourceType::GeneralWeb.as_wire_str(), "general_web");
        assert_eq!(SourceType::Docs.as_wire_str(), "docs");
        assert_eq!(SourceType::Reddit.as_wire_str(), "reddit");
    }
}
