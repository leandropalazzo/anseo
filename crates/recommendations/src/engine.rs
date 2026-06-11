//! The in-process deterministic engine + shared builder helpers.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use ulid::Ulid;

use crate::input::EngineInput;
use crate::kind::RecommendationKind;
use crate::llm::{is_deterministic_provider, EngineWarning, EnrichmentProvider, LlmConfig};
use crate::wire::{
    BenchmarkQueryRef, ConfidenceBand, LifecycleState, LlmTrace, Recommendation, Reproducibility,
    ReproducibilityClass, Severity, TimeWindow, Traceability, TAG_DETERMINISTIC_LANE,
    TAG_NON_DETERMINISTIC,
};
use crate::{kinds, llm_kinds};

/// The full result of a generation run: the Recommendation set plus any
/// top-level warnings (architecture §3.3 `warnings: []`).
#[derive(Debug, Clone)]
pub struct GenerateOutput {
    pub recommendations: Vec<Recommendation>,
    pub warnings: Vec<EngineWarning>,
}

/// Default engine semver. Bumping this changes deterministic output and is a
/// minor/major version event per §2.
pub const ENGINE_VERSION: &str = "0.5.0";

/// Wire payload truncates source-id lists at 50 (§4); the full list is at
/// `/v1/recommendations/{id}/sources`.
pub const MAX_SOURCE_IDS: usize = 50;

// Trigger thresholds (architecture §2).
pub(crate) const DOCS_MIN_RUNS: u32 = 5;
pub(crate) const RANK_BAD: f32 = 3.0;
pub(crate) const COMPETITOR_DELTA: f32 = 2.0;
pub(crate) const JACCARD_DRIFT: f32 = 0.4;
pub(crate) const COVERAGE_GRAPH_PCT: f32 = 0.10;

/// In-process recommendation engine.
#[derive(Debug, Clone)]
pub struct Engine {
    version: String,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            version: ENGINE_VERSION.to_string(),
        }
    }
}

impl Engine {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
        }
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Run every deterministic Kind producer over the input bag. Output order
    /// is fixed (Kind order × input order) so the full result is byte-stable.
    pub fn generate(&self, input: &EngineInput) -> Vec<Recommendation> {
        let mut out = Vec::new();
        out.extend(kinds::docs_not_cited_for_prompt(self, input));
        out.extend(kinds::competitor_outranks_for_prompt(self, input));
        out.extend(kinds::citation_domain_drift(self, input));
        out.extend(kinds::prompt_coverage_gap(self, input));
        out.extend(kinds::provider_blindspot(self, input));
        out.extend(kinds::low_extraction_confidence(self, input));
        out.extend(kinds::benchmark_category_underperformance(self, input));
        out
    }

    /// Full run: the deterministic set plus the LLM-aided / hybrid lane when
    /// `cfg.enrich` is set. With `enrich` but no `provider`, the deterministic
    /// set is returned unchanged plus a single `llm_enrichment_skipped` warning
    /// (architecture §3.3, `[rec-5]`).
    pub fn generate_with_llm(
        &self,
        input: &EngineInput,
        cfg: &LlmConfig,
        provider: Option<&dyn EnrichmentProvider>,
    ) -> GenerateOutput {
        let mut recommendations = self.generate(input);

        if !cfg.enrich {
            return GenerateOutput {
                recommendations,
                warnings: vec![],
            };
        }

        let Some(provider) = provider else {
            return GenerateOutput {
                recommendations,
                warnings: vec![EngineWarning::llm_enrichment_skipped(&self.version)],
            };
        };

        // Cost cap: ≤ cfg.max_recs_per_run LLM-aided recs / run (§4.4).
        let llm = llm_kinds::enrich(self, input, cfg, provider, &recommendations);
        let capped: Vec<Recommendation> = llm.into_iter().take(cfg.max_recs_per_run).collect();
        recommendations.extend(capped);
        GenerateOutput {
            recommendations,
            warnings: vec![],
        }
    }

    // ---- shared builder helpers (deterministic-lane) --------------------

    /// Assemble a deterministic-lane Recommendation. `identity` is the set of
    /// fields that make this Rec logically unique (kind + prompt/provider/
    /// competitor/etc.) — it derives both the content id and the dedup key.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_deterministic(
        &self,
        input: &EngineInput,
        kind: RecommendationKind,
        severity: Severity,
        confidence: ConfidenceBand,
        summary: String,
        payload: serde_json::Value,
        identity: &[&str],
        run_ids: Vec<Ulid>,
        citation_ids: Vec<Ulid>,
        benchmark_queries: Vec<BenchmarkQueryRef>,
    ) -> Recommendation {
        let (source_run_ids, source_run_ids_truncated) = truncate(run_ids);
        let (source_citation_ids, source_citation_ids_truncated) = truncate(citation_ids);

        let input_fingerprint = fingerprint(&serde_json::json!({
            "engine_version": self.version,
            "kind": kind.as_str(),
            "project_id": input.project_id.to_string(),
            "window": input.window,
            "payload": payload,
        }));

        let traceability = Traceability {
            source_run_ids,
            source_run_ids_truncated,
            source_citation_ids,
            source_citation_ids_truncated,
            source_benchmark_queries: benchmark_queries,
            window: input.window.clone(),
            input_fingerprint,
            llm: None,
        };

        let id = deterministic_id(kind, input.project_id, identity);

        Recommendation {
            id,
            project_id: input.project_id,
            kind,
            severity,
            confidence_band: confidence,
            state: LifecycleState::New,
            summary: clamp_summary(summary),
            payload,
            traceability,
            reproducibility: Reproducibility {
                class: ReproducibilityClass::ByteStable,
                note: None,
            },
            tags: vec![TAG_DETERMINISTIC_LANE.to_string()],
            generated_at: input.generated_at,
            engine_version: self.version.clone(),
        }
    }

    /// Assemble an LLM-aided / hybrid Recommendation. The reproducibility
    /// class + binding tag are derived from the determinism allow-list (§3.5)
    /// over the LlmTrace: allow-listed → `best_effort` + `deterministic_lane`
    /// tag; anything else → `non_deterministic` + `non_deterministic_pipeline`
    /// tag. The §5 invariant (`class == NonDeterministic ⟺ has nd tag`) holds
    /// by construction.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_llm(
        &self,
        input: &EngineInput,
        kind: RecommendationKind,
        severity: Severity,
        confidence: ConfidenceBand,
        summary: String,
        payload: serde_json::Value,
        identity: &[&str],
        run_ids: Vec<Ulid>,
        citation_ids: Vec<Ulid>,
        llm: LlmTrace,
    ) -> Recommendation {
        let (run_ids, source_run_ids_truncated) = truncate(run_ids);
        let (citation_ids, source_citation_ids_truncated) = truncate(citation_ids);

        let deterministic = is_deterministic_provider(
            &llm.provider,
            &llm.model_id,
            llm.model_content_hash.is_some(),
            llm.temperature.unwrap_or(1.0),
            llm.seed,
        );

        let input_fingerprint = fingerprint(&serde_json::json!({
            "engine_version": self.version,
            "kind": kind.as_str(),
            "project_id": input.project_id.to_string(),
            "window": input.window,
            "payload": payload,
            "provider": llm.provider,
            "model_id": llm.model_id,
        }));

        let (class, note, tag) = if deterministic {
            (
                ReproducibilityClass::BestEffort,
                Some("allow-listed provider, temperature=0 + seed pinned".to_string()),
                TAG_DETERMINISTIC_LANE,
            )
        } else {
            (
                ReproducibilityClass::NonDeterministic,
                Some("provider not on the determinism allow-list (§3.5)".to_string()),
                TAG_NON_DETERMINISTIC,
            )
        };

        Recommendation {
            id: deterministic_id(kind, input.project_id, identity),
            project_id: input.project_id,
            kind,
            severity,
            confidence_band: confidence,
            state: LifecycleState::New,
            summary: clamp_summary(summary),
            payload,
            traceability: Traceability {
                source_run_ids: run_ids,
                source_run_ids_truncated,
                source_citation_ids: citation_ids,
                source_citation_ids_truncated,
                source_benchmark_queries: vec![],
                window: input.window.clone(),
                input_fingerprint,
                llm: Some(llm),
            },
            reproducibility: Reproducibility { class, note },
            tags: vec![tag.to_string()],
            generated_at: input.generated_at,
            engine_version: self.version.clone(),
        }
    }
}

/// SHA-256 (hex, `sha256:` prefixed) of a canonical-JSON value. `serde_json`
/// serializes object keys sorted, so this is stable across runs.
pub(crate) fn fingerprint(value: &serde_json::Value) -> String {
    let bytes = value.to_string();
    let digest = Sha256::digest(bytes.as_bytes());
    let mut hex = String::with_capacity(7 + 64);
    hex.push_str("sha256:");
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

/// Content-derived id: stable across runs (no random ULID, no wall clock), and
/// doubles as the dedup identity consumed by Story 19.2's unique index.
pub(crate) fn deterministic_id(
    kind: RecommendationKind,
    project_id: Ulid,
    identity: &[&str],
) -> Ulid {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(project_id.to_string().as_bytes());
    for part in identity {
        hasher.update(b"|");
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut b16 = [0u8; 16];
    b16.copy_from_slice(&digest[..16]);
    Ulid::from(u128::from_be_bytes(b16))
}

fn truncate(mut ids: Vec<Ulid>) -> (Vec<Ulid>, bool) {
    if ids.len() > MAX_SOURCE_IDS {
        ids.truncate(MAX_SOURCE_IDS);
        (ids, true)
    } else {
        (ids, false)
    }
}

fn clamp_summary(mut s: String) -> String {
    const MAX: usize = 240;
    if s.chars().count() > MAX {
        s = s.chars().take(MAX - 1).collect::<String>();
        s.push('…');
    }
    s
}

/// Jaccard distance between two domain sets, in `[0, 1]`.
pub(crate) fn jaccard_distance(a: &[String], b: &[String]) -> f32 {
    use std::collections::BTreeSet;
    let sa: BTreeSet<&String> = a.iter().collect();
    let sb: BTreeSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        1.0 - (inter / union)
    }
}

/// Convenience: build a `TimeWindow` (used by tests + consumers).
pub fn window(start: DateTime<Utc>, end: DateTime<Utc>) -> TimeWindow {
    TimeWindow { start, end }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn jaccard_distance_known_values() {
        // Identical sets → distance 0.
        assert_eq!(jaccard_distance(&s(&["a", "b"]), &s(&["a", "b"])), 0.0);
        // Disjoint sets → distance 1.
        assert_eq!(jaccard_distance(&s(&["a"]), &s(&["b"])), 1.0);
        // {a,b} vs {b,c}: intersection 1, union 3 → 1 - 1/3 = 0.6667.
        let d = jaccard_distance(&s(&["a", "b"]), &s(&["b", "c"]));
        assert!((d - (1.0 - 1.0 / 3.0)).abs() < 1e-6, "got {d}");
    }

    #[test]
    fn jaccard_distance_dedups_and_handles_empty() {
        // Two empty sets → defined as 0 (no drift), not NaN from 0/0.
        assert_eq!(jaccard_distance(&[], &[]), 0.0);
        // Empty vs non-empty → fully drifted.
        assert_eq!(jaccard_distance(&[], &s(&["a"])), 1.0);
        // Duplicates collapse to set semantics: {a,a} == {a}.
        assert_eq!(jaccard_distance(&s(&["a", "a"]), &s(&["a"])), 0.0);
    }

    #[test]
    fn clamp_summary_truncates_with_ellipsis_and_keeps_short_intact() {
        // A short summary passes through byte-for-byte.
        let short = "all good".to_string();
        assert_eq!(clamp_summary(short.clone()), short);

        // A 300-char summary is clamped to exactly 240 chars, last char the …
        let long = "x".repeat(300);
        let clamped = clamp_summary(long);
        assert_eq!(clamped.chars().count(), 240);
        assert!(clamped.ends_with('…'));
        assert_eq!(clamped.chars().filter(|c| *c == 'x').count(), 239);
    }

    #[test]
    fn clamp_summary_counts_chars_not_bytes() {
        // Multi-byte chars must not blow the 240-CHAR budget nor panic on a
        // byte-boundary slice.
        let long = "é".repeat(300);
        let clamped = clamp_summary(long);
        assert_eq!(clamped.chars().count(), 240);
        assert!(clamped.ends_with('…'));
    }

    #[test]
    fn truncate_caps_at_max_source_ids_and_flags() {
        // Under the cap: untouched, not flagged.
        let few: Vec<Ulid> = (0..3).map(|_| Ulid::new()).collect();
        let (kept, flagged) = truncate(few.clone());
        assert_eq!(kept, few);
        assert!(!flagged);

        // Over the cap: truncated to MAX_SOURCE_IDS and flagged.
        let many: Vec<Ulid> = (0..MAX_SOURCE_IDS + 7).map(|_| Ulid::new()).collect();
        let (kept, flagged) = truncate(many.clone());
        assert_eq!(kept.len(), MAX_SOURCE_IDS);
        assert!(flagged);
        // Truncation keeps the prefix (the first MAX_SOURCE_IDS ids), in order.
        assert_eq!(kept, &many[..MAX_SOURCE_IDS]);

        // Exactly at the cap: kept whole, NOT flagged (boundary is `>`).
        let exact: Vec<Ulid> = (0..MAX_SOURCE_IDS).map(|_| Ulid::new()).collect();
        let (kept, flagged) = truncate(exact.clone());
        assert_eq!(kept.len(), MAX_SOURCE_IDS);
        assert!(!flagged);
    }

    #[test]
    fn fingerprint_is_prefixed_stable_and_input_sensitive() {
        let a = fingerprint(&serde_json::json!({ "k": 1, "j": 2 }));
        // sha256: prefix + 64 hex chars.
        assert!(a.starts_with("sha256:"));
        assert_eq!(a.len(), "sha256:".len() + 64);
        // Key order in the source JSON does not matter (serde sorts keys), so
        // the same logical value fingerprints identically.
        let b = fingerprint(&serde_json::json!({ "j": 2, "k": 1 }));
        assert_eq!(a, b);
        // A different value yields a different digest.
        let c = fingerprint(&serde_json::json!({ "k": 1, "j": 3 }));
        assert_ne!(a, c);
    }

    #[test]
    fn deterministic_id_is_content_derived_and_identity_sensitive() {
        let pid = Ulid::new();
        let id1 = deterministic_id(
            RecommendationKind::ProviderBlindspot,
            pid,
            &["p1", "openai"],
        );
        // Same kind + project + identity → same id (no wall clock, no randomness).
        let id2 = deterministic_id(
            RecommendationKind::ProviderBlindspot,
            pid,
            &["p1", "openai"],
        );
        assert_eq!(id1, id2);
        // Different identity → different id.
        let id3 = deterministic_id(
            RecommendationKind::ProviderBlindspot,
            pid,
            &["p1", "gemini"],
        );
        assert_ne!(id1, id3);
        // Different kind → different id (kind is part of the hash preimage).
        let id4 = deterministic_id(
            RecommendationKind::PromptCoverageGap,
            pid,
            &["p1", "openai"],
        );
        assert_ne!(id1, id4);
    }

    #[test]
    fn window_helper_builds_the_pair() {
        let start = Utc::now();
        let end = start + chrono::Duration::days(14);
        let w = window(start, end);
        assert_eq!(w.start, start);
        assert_eq!(w.end, end);
    }
}
