// ATDD red-phase smoke for Epic 1 / Story 1: Execute Prompt Run against a Provider.
//
// Mapped story: _bmad-output/planning-artifacts/stories/epic-1-story-1.md
// Mapped ACs: AC-1..AC-5
// Mapped FRs: FR-2, NFR-2, NFR-4, NFR-17
//
// Full scaffold (with sqlx::test + MockProvider) lives in:
//   _bmad-output/test-artifacts/atdd-checklist-epic-1-story-1.md
//
// This on-disk smoke compiles cleanly so `cargo test --workspace --no-run` succeeds,
// but fails at runtime so `cargo test --workspace` shows ≥ 1 red-phase failure.
// Dev replaces this body (or this whole file) with the full scaffold once
// `crates/core::testing::MockProvider`, `Orchestrator`, and migration 0001
// land per the checklist's "To turn this green, dev must land" section.

#[test]
#[ignore = "red-phase ATDD placeholder for a later prompt-run story; Story 1.1 only requires skeleton buildability"]
fn ac_1_one_run_per_prompt_x_provider_unimplemented() {
    // AC-1 (Epic 1 Story 1): with 2 Prompts × 2 Providers, Orchestrator::run_all
    // must produce exactly 4 prompt_runs rows in Postgres.
    //
    // RED-PHASE — replaced with the full sqlx::test scaffold (see atdd-checklist-epic-1-story-1.md)
    // once Orchestrator + MockProvider + migration 0001 land.
    panic!(
        "AC-1 unimplemented: orchestrator + MockProvider + migration 0001 not yet built. \
         See _bmad-output/test-artifacts/atdd-checklist-epic-1-story-1.md."
    );
}
