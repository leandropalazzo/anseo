//! Tracing redaction integration (P0-021, R-002).
//!
//! Asserts the contract between `opengeo_core::Secret` and the `tracing`
//! stack: when a `Secret`-wrapped value is logged via the `?` / `%` field
//! formatters, the structured log output must contain `[REDACTED]` and must
//! NOT contain the raw secret material.
//!
//! Coverage scope:
//!  - `tracing::info!(secret = ?secret, ...)` — `Debug` path
//!  - `tracing::info!(secret = %secret, ...)` — `Display` path
//!  - Negative-control: raw strings passed without the `Secret` wrapper are
//!    NOT redacted by the tracing stack today. This is an intentional pin —
//!    the contract is "wrap your secrets in `Secret`", not "the tracing
//!    layer scrubs arbitrary strings." If that contract changes (e.g., a
//!    redactor layer is added), update this test alongside it.
//!
//! trace: P0-020 (R-002 Debug redaction — integration)
//! trace: P0-021 (R-002 tracing field redaction — integration)

use opengeo_core::Secret;
use tracing::info;
use tracing_test::traced_test;

const FIXTURE_API_KEY: &str = "sk-very-secret-12345";
const FIXTURE_BEARER: &str = "Bearer abc.def.ghi-do-not-log";

#[traced_test]
#[test]
fn debug_formatter_redacts_secret_via_tracing() {
    let api_key = Secret::new(FIXTURE_API_KEY);
    info!(api_key = ?api_key, "issuing request");

    assert!(
        logs_contain("[REDACTED]"),
        "tracing output should include [REDACTED] marker for Debug-formatted Secret"
    );
    assert!(
        !logs_contain(FIXTURE_API_KEY),
        "tracing output must NOT leak the raw secret via Debug formatting"
    );
}

#[traced_test]
#[test]
fn display_formatter_redacts_secret_via_tracing() {
    let bearer = Secret::new(FIXTURE_BEARER);
    info!(auth = %bearer, "applying authorization header");

    assert!(
        logs_contain("[REDACTED]"),
        "tracing output should include [REDACTED] marker for Display-formatted Secret"
    );
    assert!(
        !logs_contain(FIXTURE_BEARER),
        "tracing output must NOT leak the raw secret via Display formatting"
    );
    assert!(
        !logs_contain("abc.def.ghi-do-not-log"),
        "tracing output must NOT leak the token suffix"
    );
}

#[traced_test]
#[test]
fn nested_struct_with_secret_field_redacts_in_debug() {
    // Realistic case: a request/config struct that contains a Secret. As long
    // as the surrounding struct derives Debug normally and the Secret field
    // is logged through the same Debug path, the secret stays redacted.
    #[allow(dead_code)] // fields read only via Debug
    #[derive(Debug)]
    struct ProviderConfig {
        provider: &'static str,
        model: &'static str,
        api_key: Secret,
    }

    let cfg = ProviderConfig {
        provider: "openai",
        model: "gpt-4o-2024-08-06",
        api_key: Secret::new(FIXTURE_API_KEY),
    };

    info!(config = ?cfg, "loaded provider config");

    assert!(
        logs_contain("openai"),
        "non-secret fields should appear normally"
    );
    assert!(logs_contain("[REDACTED]"));
    assert!(
        !logs_contain(FIXTURE_API_KEY),
        "raw secret must not leak even when nested inside another Debug struct"
    );
}

#[traced_test]
#[test]
fn raw_unwrapped_string_is_not_redacted_pinning_current_contract() {
    // PIN: the redaction contract today is "wrap secrets in `Secret`." A bare
    // `&str` passed to tracing IS logged verbatim. If/when a redactor layer
    // scrubs `sk-*` and `Bearer ` patterns from arbitrary strings, this test
    // should flip to assert redaction instead — and the source-of-truth
    // ProviderError taxonomy + adapters should be audited for any place we
    // construct error messages with raw token material.
    let raw = FIXTURE_API_KEY;
    info!(raw_input = raw, "logging an unwrapped string on purpose");

    assert!(
        logs_contain(FIXTURE_API_KEY),
        "current contract: raw strings are not redacted by the tracing layer"
    );
}
