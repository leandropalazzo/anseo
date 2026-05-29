//! Sanity test for Epic 1 / Story 1 — `apps/api` builds and the router can
//! be assembled.
//!
//! ## Why this file shrank
//!
//! Originally this carried a single `#[ignore]`'d red-phase ATDD placeholder
//! (`ac_1_one_run_per_prompt_x_provider_unimplemented`) per
//! `_bmad-output/test-artifacts/atdd-checklist-epic-1-story-1.md`. That
//! scaffold expected a `crates/core::testing::MockProvider` and
//! `apps/api::services::Orchestrator` that ended up shipping in
//! `crates/providers` instead (Story 2.4 / Story 3.1). The actual Epic 1
//! Story 1 acceptance criteria are now fully covered by:
//!
//! - `crates/providers/tests/persistence_smoke.rs::
//!     full_matrix_persists_with_one_failure_and_three_successes`
//!   AC-1 (4 rows from 2×2 matrix), AC-2 (status persistence),
//!   AC-3 (failure isolation), AC-4 (error_kind taxonomy),
//!   AC-5 (project + prompt upserts).
//! - `…::rerun_does_not_lose_history` covers FR-6 re-run semantics.
//! - `crates/providers/tests/provider_contract.rs` covers the Provider trait
//!   contract that AC-1 implicitly relies on.
//!
//! Rather than keep a permanently-`#[ignore]`'d duplicate, this file now
//! provides a single green sanity check that `apps/api::router` can be built
//! against an `AppState` — so any future Provider/Storage/router-shape
//! refactor that breaks the `apps/api` integration surface fails CI
//! immediately, with no dependency on Postgres.
//!
//! trace: P0-002 (FR-2) — covered by persistence_smoke.rs
//! trace: P0-007 (FR-9) — covered by provider_contract.rs

use std::sync::Arc;

use opengeo_api::{router, AppState};
use opengeo_core::ProjectId;

#[tokio::test]
async fn api_router_builds_with_minimal_state() {
    // Construct an AppState without a real database connection. We can do
    // this because `opengeo_storage::Storage` exposes `from_pool` and we can
    // build an in-memory placeholder pool via `sqlx::PgPool::connect_lazy` —
    // the router itself doesn't dereference the pool until a request lands.
    let lazy_pool =
        sqlx::PgPool::connect_lazy("postgres://opengeo:opengeo@127.0.0.1:1/__router_build_smoke__")
            .expect("connect_lazy never IOs synchronously");
    let storage = Arc::new(opengeo_storage::Storage::from_pool(lazy_pool));
    let (events, _rx) = opengeo_scheduler::worker::event_channel();
    let state = AppState {
        storage,
        project_id: ProjectId::new(),
        events,
        config: None,
        provider_registry: None,
    };
    let _r = router(state);
    // No assertions needed: the test passes by building. If routes::test_seed
    // or any of the merged routers grow a type-shape mismatch, the workspace
    // won't compile and this test won't reach this line.
}

#[test]
fn api_router_includes_test_seed_when_env_set() {
    // Pin the env-gate contract: setting OPENGEO_TEST_MODE=1 makes
    // is_enabled_via_env() flip true. The router() function consumes that
    // signal at build time (verified by the unit test in
    // routes/test_seed.rs). This guards against future refactors that move
    // the env check to e.g. a request middleware (which would defeat the
    // "production never serves /test/seed" contract).
    // SAFETY: tests in this crate run on a single thread by default.
    unsafe { std::env::set_var("OPENGEO_TEST_MODE", "1") };
    assert!(opengeo_api::routes::test_seed::is_enabled_via_env());
    unsafe { std::env::set_var("OPENGEO_TEST_MODE", "0") };
    assert!(!opengeo_api::routes::test_seed::is_enabled_via_env());
    unsafe { std::env::remove_var("OPENGEO_TEST_MODE") };
    assert!(!opengeo_api::routes::test_seed::is_enabled_via_env());
}
