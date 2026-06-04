//! Phase 2 Story 14.5 — Phase 1 contract freeze.
//!
//! Pins the load-bearing wire shapes Phase 1 acceptance tests depend on
//! so a Phase 2 refactor can't silently regress them. Each assertion
//! here is intentionally redundant with the per-module unit tests —
//! the value of this file is that ONE diff surface fails when a
//! contract changes, instead of the same change rippling through a
//! dozen unrelated test files.
//!
//! ## Contract surfaces frozen
//!
//! 1. **Provider Error Taxonomy** — the closed enum at
//!    `ProviderErrorKind`. Removal of a variant is a hard regression
//!    (Phase 1 CLIs grepping on the wire string break). Addition is
//!    allowed and tested separately.
//! 2. **Exit codes** — every `ExitCode` constant pinned. PRD §11.4
//!    locks these for CI consumers.
//! 3. **Default model names** — Phase 1 wired specific OpenAI +
//!    Anthropic defaults into the schedule cost projection. Drift
//!    silently changes monthly cost numbers.
//! 4. **YAML schema versions** — v0.1 (Phase 1) and v0.2 (Phase 2)
//!    constants. A future tweak that bumps v0.1 → v0.3 (skipping v0.2)
//!    would lose v0.2 backward compatibility silently.

use opengeo_core::{
    ExitCode, ProviderErrorKind, DEFAULT_ANTHROPIC_MODEL, DEFAULT_OPENAI_MODEL,
    SCHEMA_VERSION_V0_1, SCHEMA_VERSION_V0_2,
};

#[test]
fn provider_error_kind_phase1_variants_remain_present() {
    // Each of the 6 Phase 1 variants must be reachable AND
    // serialize to the documented wire string. Removing one breaks
    // the CHECK constraint in migration 20260525120000_initial.sql
    // (`prompt_runs.error_kind IN (...)`) and the Phase 1 CLI's
    // exit-code mapping.
    let pairs = [
        (
            ProviderErrorKind::ProviderUnauthorized,
            "provider_unauthorized",
        ),
        (
            ProviderErrorKind::ProviderRateLimited,
            "provider_rate_limited",
        ),
        (ProviderErrorKind::ProviderTimeout, "provider_timeout"),
        (ProviderErrorKind::Provider5xx, "provider_5xx"),
        (
            ProviderErrorKind::ProviderInvalidResponse,
            "provider_invalid_response",
        ),
        (ProviderErrorKind::NetworkError, "network_error"),
    ];
    for (variant, wire) in pairs {
        assert_eq!(
            variant.as_wire_str(),
            wire,
            "variant {variant:?} must serialize to `{wire}` (PRD §11.5)"
        );
    }
}

#[test]
fn provider_error_kind_accepts_phase2_additive_variant() {
    // Phase 2 adds ProviderUnsupportedModel additively. The contract
    // freeze allows additions; the migration widened the CHECK
    // constraint in 20260528120000_schedules_and_webhooks.sql.
    assert_eq!(
        ProviderErrorKind::ProviderUnsupportedModel.as_wire_str(),
        "provider_unsupported_model"
    );
}

#[test]
fn exit_codes_pin_prd_11_4() {
    // PRD §11.4 / `_bmad-output/planning-artifacts/architecture.md`:
    // CI consumers depend on these specific integer values.
    assert_eq!(ExitCode::Success as i32, 0);
    assert_eq!(ExitCode::VisibilityCheckFailed as i32, 1);
    assert_eq!(ExitCode::ProviderError as i32, 2);
    assert_eq!(ExitCode::ConfigError as i32, 64);
    assert_eq!(ExitCode::DataError as i32, 65);
    assert_eq!(ExitCode::AuthError as i32, 66);
    assert_eq!(ExitCode::InternalError as i32, 70);
}

#[test]
fn schema_versions_remain_v0_1_and_v0_2() {
    // Bumping v0.1 → v0.3 (skipping v0.2) would silently break
    // backwards compatibility for any deployed v0.1 config. The
    // Phase 2 promise was "v0.2 is a non-breaking superset of v0.1".
    assert_eq!(SCHEMA_VERSION_V0_1, "0.1");
    assert_eq!(SCHEMA_VERSION_V0_2, "0.2");
}

#[test]
fn default_models_phase1_remain_specific() {
    // Phase 1 schedule cost projection uses these specific defaults.
    // A silent bump (e.g., gpt-4o-mini becoming the OpenAI default)
    // changes every operator's monthly cost projection without
    // warning. The contract: any change MUST come with a migration
    // note + a version bump documented in the release manual.
    assert_eq!(DEFAULT_OPENAI_MODEL, "gpt-4o-2024-08-06");
    assert_eq!(DEFAULT_ANTHROPIC_MODEL, "claude-3-5-sonnet-20241022");
}

#[test]
fn provider_error_kind_round_trips_through_serde() {
    // The wire-stable strings are what crosses the JSON boundary.
    // Round-trip a representative subset.
    for variant in [
        ProviderErrorKind::ProviderUnauthorized,
        ProviderErrorKind::Provider5xx,
        ProviderErrorKind::NetworkError,
        ProviderErrorKind::ProviderUnsupportedModel,
    ] {
        let json = serde_json::to_string(&variant).unwrap();
        let back: ProviderErrorKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, variant, "round-trip lost {variant:?}");
    }
}

#[test]
fn provider_error_kind_wire_format_is_snake_case() {
    // The serde rename rule applied to the enum is `snake_case`. A
    // future rename to camelCase would break every downstream JSON
    // consumer in lockstep.
    let json = serde_json::to_string(&ProviderErrorKind::ProviderInvalidResponse).unwrap();
    assert_eq!(json, "\"provider_invalid_response\"");
}
