use serde::Deserialize;
use std::collections::BTreeSet;

use crate::TERMS_VERSION;

const CANONICAL_GEO_PROMPT_SUITE_RAW: &str =
    include_str!("../data/canonical_geo_prompt_suite.v1.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CanonicalPromptSuite {
    pub suite_id: String,
    pub suite_version: String,
    pub terms_version: String,
    pub ownership: SuiteOwnership,
    pub entries: Vec<CanonicalPromptEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SuiteOwnership {
    pub owner: String,
    pub change_control: String,
    pub deprecation_policy: String,
    pub review_notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CanonicalPromptEntry {
    pub slug: String,
    pub version: String,
    pub category: String,
    pub description: String,
    pub contribution_cohort: String,
    pub cohort_description: String,
    pub prompt_template: String,
    pub deprecated: bool,
}

pub fn canonical_geo_prompt_suite() -> &'static CanonicalPromptSuite {
    static SUITE: std::sync::OnceLock<CanonicalPromptSuite> = std::sync::OnceLock::new();
    SUITE.get_or_init(|| {
        let parsed: CanonicalPromptSuite =
            serde_json::from_str(CANONICAL_GEO_PROMPT_SUITE_RAW).expect("suite JSON must parse");
        validate_suite(&parsed).expect("suite JSON must satisfy invariants");
        parsed
    })
}

pub fn canonical_prompt_by_slug(slug: &str) -> Option<&'static CanonicalPromptEntry> {
    canonical_geo_prompt_suite()
        .entries
        .iter()
        .find(|entry| entry.slug == slug)
}

fn validate_suite(suite: &CanonicalPromptSuite) -> Result<(), String> {
    if suite.entries.is_empty() {
        return Err("canonical suite must not be empty".into());
    }
    if suite.terms_version != TERMS_VERSION {
        return Err(format!(
            "suite terms_version `{}` must match benchmark TERMS_VERSION `{TERMS_VERSION}`",
            suite.terms_version
        ));
    }
    if suite.suite_id.trim().is_empty() || suite.suite_version.trim().is_empty() {
        return Err("suite_id and suite_version must be non-empty".into());
    }

    let mut seen_slugs = BTreeSet::new();
    for entry in &suite.entries {
        if entry.slug.trim().is_empty()
            || entry.version.trim().is_empty()
            || entry.category.trim().is_empty()
            || entry.description.trim().is_empty()
            || entry.contribution_cohort.trim().is_empty()
            || entry.cohort_description.trim().is_empty()
            || entry.prompt_template.trim().is_empty()
        {
            return Err(format!(
                "suite entry `{}` has an empty required field",
                entry.slug
            ));
        }
        if entry.version != suite.suite_id {
            return Err(format!(
                "suite entry `{}` version `{}` must equal suite_id `{}`",
                entry.slug, entry.version, suite.suite_id
            ));
        }
        if !entry.slug.starts_with(&format!("{}/", suite.suite_id)) {
            return Err(format!(
                "suite entry `{}` must be namespaced under `{}/`",
                entry.slug, suite.suite_id
            ));
        }
        if !is_slug_safe(&entry.slug) {
            return Err(format!(
                "suite entry `{}` is not slug-safe (lowercase ASCII + digits + hyphens + /)",
                entry.slug
            ));
        }
        if !seen_slugs.insert(entry.slug.clone()) {
            return Err(format!("duplicate suite slug `{}`", entry.slug));
        }
    }

    Ok(())
}

fn is_slug_safe(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_loads_with_non_empty_entries() {
        let suite = canonical_geo_prompt_suite();
        assert_eq!(suite.suite_id, "geo-v1");
        assert_eq!(suite.terms_version, TERMS_VERSION);
        assert!(!suite.entries.is_empty());
    }

    #[test]
    fn suite_slugs_are_unique_and_namespaced() {
        let suite = canonical_geo_prompt_suite();
        let mut seen = BTreeSet::new();
        for entry in &suite.entries {
            assert!(entry.slug.starts_with("geo-v1/"));
            assert!(
                seen.insert(entry.slug.clone()),
                "duplicate slug {}",
                entry.slug
            );
        }
    }

    #[test]
    fn suite_change_control_is_explicit() {
        let suite = canonical_geo_prompt_suite();
        assert_eq!(
            suite.ownership.change_control,
            "additive-only-within-version"
        );
        assert!(suite
            .ownership
            .deprecation_policy
            .contains("not repurposed"));
    }

    #[test]
    fn suite_entries_cover_multiple_categories() {
        let suite = canonical_geo_prompt_suite();
        let categories = suite
            .entries
            .iter()
            .map(|entry| entry.category.as_str())
            .collect::<BTreeSet<_>>();
        assert!(categories.len() >= 4, "expected category diversity");
    }

    #[test]
    fn current_geo_v1_entries_keep_stable_metadata() {
        let suite = canonical_geo_prompt_suite();
        let expected = [
            (
                "geo-v1/best-vector-db",
                "platform-selection",
                "geo-v1:platform-selection",
            ),
            (
                "geo-v1/best-rag-platform",
                "platform-selection",
                "geo-v1:platform-selection",
            ),
            (
                "geo-v1/llm-observability-tools",
                "observability",
                "geo-v1:observability",
            ),
            (
                "geo-v1/ai-search-visibility-platforms",
                "brand-visibility",
                "geo-v1:brand-visibility",
            ),
            (
                "geo-v1/best-enterprise-chatbot-platform",
                "application-platform",
                "geo-v1:application-platform",
            ),
            (
                "geo-v1/agent-frameworks",
                "developer-frameworks",
                "geo-v1:developer-frameworks",
            ),
            (
                "geo-v1/best-ai-evaluation-tools",
                "evaluation",
                "geo-v1:evaluation",
            ),
            (
                "geo-v1/customer-support-ai",
                "use-case-solutions",
                "geo-v1:use-case-solutions",
            ),
        ];

        for (slug, category, contribution_cohort) in expected {
            let entry = suite
                .entries
                .iter()
                .find(|entry| entry.slug == slug)
                .unwrap_or_else(|| panic!("missing canonical suite entry `{slug}`"));
            assert_eq!(entry.version, "geo-v1");
            assert_eq!(entry.category, category);
            assert_eq!(entry.contribution_cohort, contribution_cohort);
        }
    }

    #[test]
    fn lookup_returns_matching_entry_when_slug_is_canonical() {
        let entry = canonical_prompt_by_slug("geo-v1/best-vector-db")
            .expect("known canonical slug should resolve");
        assert_eq!(entry.version, "geo-v1");
        assert_eq!(entry.category, "platform-selection");
    }

    #[test]
    fn lookup_returns_none_for_unknown_slug() {
        assert!(canonical_prompt_by_slug("custom/not-canonical").is_none());
    }
}
