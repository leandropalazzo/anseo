//! Story 19.6 — assemble an [`EngineInput`] from live project data.
//!
//! The engine itself never touches a DB (AD-Phase3-RecommendationsInProcess):
//! the API consumer fetches the project's prompts / runs / citations, projects
//! them into the plain [`ProjectFacts`] bag here, and this module folds them
//! into the engine's [`EngineInput`]. Keeping the fold a pure function over
//! plain data (no `sqlx`, no `Storage`) is what makes it unit-testable offline
//! and keeps the byte-stable contract honest — the same facts always assemble
//! the same input.
//!
//! Visibility ranks and benchmark category slices come from ClickHouse /
//! benchmark aggregates which are wired separately; until then the assembled
//! input carries empty rank/benchmark data, so the rank- and benchmark-gated
//! Kinds simply do not fire. The Postgres-derived signals (run counts, citation
//! domains, evidence ids) are populated for real.

use chrono::{DateTime, Utc};
use ulid::Ulid;

use crate::engine::ENGINE_VERSION;
use crate::input::{EngineInput, PromptStat};
use crate::wire::TimeWindow;

/// One Prompt Run's contribution to the window, projected from Postgres.
#[derive(Debug, Clone)]
pub struct PromptRunFacts {
    pub run_id: Ulid,
    /// eTLD+1 domains the run's citations resolved to.
    pub citation_domains: Vec<String>,
    pub citation_ids: Vec<Ulid>,
}

/// A tracked Prompt and the runs observed in the window.
#[derive(Debug, Clone)]
pub struct PromptFacts {
    pub prompt_id: Ulid,
    pub prompt: String,
    pub runs: Vec<PromptRunFacts>,
}

/// The full live-data bag the API handler assembles before generation.
#[derive(Debug, Clone)]
pub struct ProjectFacts {
    pub project_id: Ulid,
    pub brand: String,
    pub brand_etld1: String,
    pub docs_etld1: Option<String>,
    pub competitors: Vec<String>,
    pub enabled_providers: Vec<String>,
    pub benchmark_opted_in: bool,
    pub prompts: Vec<PromptFacts>,
    pub window: TimeWindow,
    /// Passed in (never `now()`) so a replay over the same facts is byte-stable.
    pub generated_at: DateTime<Utc>,
}

/// Default extraction promotion threshold (closes Phase 2 OQ-21), mirrored from
/// the engine's deterministic defaults.
const DEFAULT_EXTRACTION_THRESHOLD: f32 = 0.6;

/// Fold live [`ProjectFacts`] into the engine's [`EngineInput`]. Pure: the same
/// facts always yield the same input (modulo the caller-supplied `generated_at`).
pub fn assemble(facts: ProjectFacts) -> EngineInput {
    let mut all_run_ids: Vec<Ulid> = Vec::new();

    let prompts = facts
        .prompts
        .into_iter()
        .map(|p| {
            let mut run_ids = Vec::with_capacity(p.runs.len());
            let mut citation_ids = Vec::new();
            let mut brand_citation_domains: Vec<String> = Vec::new();
            for run in &p.runs {
                run_ids.push(run.run_id);
                all_run_ids.push(run.run_id);
                citation_ids.extend(run.citation_ids.iter().copied());
                for d in &run.citation_domains {
                    if !brand_citation_domains.contains(d) {
                        brand_citation_domains.push(d.clone());
                    }
                }
            }
            let n_runs_14d = run_ids.len() as u32;
            PromptStat {
                prompt_id: p.prompt_id,
                prompt: p.prompt,
                run_ids,
                citation_ids,
                n_runs_14d,
                brand_mean_rank: None,
                provider_ranks: Vec::new(),
                competitor_ranks: Vec::new(),
                brand_citation_domains,
                competitor_docs_present: Vec::new(),
            }
        })
        .collect();

    EngineInput {
        project_id: facts.project_id,
        brand: facts.brand,
        brand_etld1: facts.brand_etld1,
        docs_etld1: facts.docs_etld1,
        competitors: facts.competitors,
        enabled_providers: facts.enabled_providers,
        benchmark_opted_in: facts.benchmark_opted_in,
        extraction_p50: None,
        extraction_threshold: DEFAULT_EXTRACTION_THRESHOLD,
        extraction_suggested_plugin: String::new(),
        prompts,
        citation_drift: None,
        benchmark_categories: Vec::new(),
        window: facts.window,
        generated_at: facts.generated_at,
        engine_version: ENGINE_VERSION.to_string(),
        all_run_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn window() -> TimeWindow {
        TimeWindow {
            start: Utc.with_ymd_and_hms(2026, 5, 16, 0, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 5, 30, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn folds_runs_and_citations_into_prompt_stats() {
        let r1 = Ulid::new();
        let r2 = Ulid::new();
        let c1 = Ulid::new();
        let c2 = Ulid::new();
        let facts = ProjectFacts {
            project_id: Ulid::new(),
            brand: "Acme".into(),
            brand_etld1: "acme.com".into(),
            docs_etld1: Some("docs.acme.com".into()),
            competitors: vec!["Globex".into()],
            enabled_providers: vec!["openai".into()],
            benchmark_opted_in: false,
            prompts: vec![PromptFacts {
                prompt_id: Ulid::new(),
                prompt: "best crm".into(),
                runs: vec![
                    PromptRunFacts {
                        run_id: r1,
                        citation_domains: vec!["g2.com".into(), "g2.com".into()],
                        citation_ids: vec![c1],
                    },
                    PromptRunFacts {
                        run_id: r2,
                        citation_domains: vec!["capterra.com".into()],
                        citation_ids: vec![c2],
                    },
                ],
            }],
            window: window(),
            generated_at: Utc.with_ymd_and_hms(2026, 5, 30, 12, 0, 0).unwrap(),
        };

        let input = assemble(facts);
        assert_eq!(input.brand, "Acme");
        assert_eq!(input.engine_version, ENGINE_VERSION);
        assert_eq!(input.all_run_ids, vec![r1, r2]);
        assert_eq!(input.prompts.len(), 1);
        let p = &input.prompts[0];
        assert_eq!(p.n_runs_14d, 2);
        assert_eq!(p.run_ids, vec![r1, r2]);
        assert_eq!(p.citation_ids, vec![c1, c2]);
        // Domains de-duplicated, insertion order preserved.
        assert_eq!(p.brand_citation_domains, vec!["g2.com", "capterra.com"]);
    }

    #[test]
    fn empty_project_assembles_empty_input() {
        let facts = ProjectFacts {
            project_id: Ulid::new(),
            brand: "Acme".into(),
            brand_etld1: "acme.com".into(),
            docs_etld1: None,
            competitors: vec![],
            enabled_providers: vec![],
            benchmark_opted_in: false,
            prompts: vec![],
            window: window(),
            generated_at: Utc.with_ymd_and_hms(2026, 5, 30, 12, 0, 0).unwrap(),
        };
        let input = assemble(facts);
        assert!(input.prompts.is_empty());
        assert!(input.all_run_ids.is_empty());
        assert_eq!(input.extraction_threshold, DEFAULT_EXTRACTION_THRESHOLD);
    }
}
