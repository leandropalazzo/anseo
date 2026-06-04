//! OpenGEO GEO Recommendations engine (FR-58) — Story 19.1.
//!
//! In-process, deterministic engine over a pre-aggregated [`input::EngineInput`]
//! bag. Story 19.1 ships the seven **deterministic** Kinds (architecture
//! §2.1–2.6, 2.9); the LLM-aided / hybrid Kinds are declared in [`kind`] and
//! produced by Story 19.3. Live data-source wiring is a consumer concern
//! (Story 19.6).
//!
//! Two committed contracts, both unit-tested:
//! - **`[rec-1]` determinism:** generating twice against the same fixture
//!   yields a byte-identical wire payload (content-derived ids + passed-in
//!   `generated_at`; no random ULIDs, no wall clock).
//! - **`[rec-6]` traceability:** every Recommendation carries real evidence
//!   ([`wire::Traceability::is_non_empty`]); empty traceability is a hard bug.

pub mod assembly;
pub mod engine;
pub mod input;
pub mod kind;
pub mod kinds;
pub mod lifecycle;
pub mod llm;
pub mod llm_kinds;
pub mod wire;

pub use assembly::{assemble, ProjectFacts, PromptFacts, PromptRunFacts};
pub use engine::{window, Engine, GenerateOutput, ENGINE_VERSION};
pub use input::{
    BenchmarkCategory, CitationDriftInput, CompetitorRank, EngineInput, PromptStat, ProviderRank,
};
pub use kind::{Lane, RecommendationKind};
pub use lifecycle::{
    can_transition, mark_acted, measure_outcome, outcome_due_at, transition, LifecycleError,
    LifecycleWarning, MarkActedResult, MeasurementOutcome, OutcomeStatus, State,
    MIN_POST_ACTED_RUNS, OUTCOME_WINDOW_DAYS,
};
pub use llm::{
    is_deterministic_provider, EngineWarning, EnrichmentError, EnrichmentOutcome,
    EnrichmentProvider, EnrichmentRequest, LlmConfig, StubProvider,
};
pub use wire::{
    BenchmarkQueryRef, ConfidenceBand, LifecycleState, LlmTrace, Recommendation, Reproducibility,
    ReproducibilityClass, Severity, TimeWindow, Traceability, TAG_DETERMINISTIC_LANE,
    TAG_NON_DETERMINISTIC,
};
