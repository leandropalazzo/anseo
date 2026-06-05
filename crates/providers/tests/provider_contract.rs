//! Provider trait contract (P0-007, FR-9).
//!
//! Every `Provider` implementation must satisfy the same observable contract.
//! This file defines the scenarios once and runs them against `MockProvider`;
//! `crates/providers/tests/wiremock_smoke.rs` exercises the same contract
//! shape against the real `OpenAiProvider` and `AnthropicProvider` adapters
//! (via `wiremock` for the HTTP layer). Between the two files, every
//! Phase 1 Provider impl is contract-tested.
//!
//! The contract:
//! 1. `name()` is stable and matches the `ProviderName` identity used at
//!    construction.
//! 2. `validate_model(known_model)` returns `Ok(canonical_model_string)`.
//! 3. `validate_model(unknown_model)` returns `Err(...)` with kind
//!    `ProviderInvalidResponse`, *before* any network call.
//! 4. `run(request)` returns a `ProviderResponse` whose `provider` field
//!    equals `self.name()`.
//! 5. Provider-side failures map into the closed `ProviderErrorKind` taxonomy
//!    — adapters must never let raw `reqwest::Error`/`serde_json::Error`
//!    types leak through the trait surface.
//!
//! trace: P0-007 (FR-9)

use anseo_core::ProviderErrorKind;
use anseo_providers::{
    mock::MockProvider, Provider, ProviderError, ProviderRequest, ProviderResponse,
};

// ---- Contract scenarios (parameterized over any `&dyn Provider`) -----------

async fn contract_name_is_stable(provider: &dyn Provider, expected: anseo_core::ProviderName) {
    assert_eq!(
        provider.name(),
        expected,
        "Provider::name() must be the identity passed at construction"
    );
}

fn contract_validate_known_model_ok(provider: &dyn Provider, model: &str) {
    let result = provider.validate_model(model);
    assert!(
        result.is_ok(),
        "validate_model({model}) should be Ok for a known model, got {result:?}"
    );
    assert_eq!(
        result.unwrap(),
        model,
        "validate_model must echo the canonical model string"
    );
}

fn contract_validate_unknown_model_errors_pre_flight(provider: &dyn Provider, unknown: &str) {
    let err = provider
        .validate_model(unknown)
        .expect_err("validate_model must reject unknown models");
    assert_eq!(
        err.kind,
        ProviderErrorKind::ProviderInvalidResponse,
        "unknown-model validation must use the ProviderInvalidResponse variant",
    );
}

async fn contract_run_response_carries_provider_identity(
    provider: &dyn Provider,
    request: ProviderRequest,
    expected_name: anseo_core::ProviderName,
) -> ProviderResponse {
    let response = provider
        .run(request)
        .await
        .expect("queued success should return a response");
    assert_eq!(
        response.provider, expected_name,
        "ProviderResponse.provider must match Provider::name()"
    );
    response
}

async fn contract_run_failure_maps_to_closed_taxonomy(
    provider: &dyn Provider,
    request: ProviderRequest,
    expected_kind: ProviderErrorKind,
) {
    let err = provider
        .run(request)
        .await
        .expect_err("queued failure should propagate a ProviderError");
    assert_eq!(
        err.kind, expected_kind,
        "Provider::run errors must map into the closed ProviderErrorKind taxonomy",
    );
}

// ---- Run the contract against MockProvider --------------------------------

#[tokio::test]
async fn mock_provider_satisfies_contract_for_openai_identity() {
    let provider = MockProvider::new(anseo_core::ProviderName::Openai)
        .accept_model("mock-model")
        .queue_response("contract test response");

    contract_name_is_stable(&provider, anseo_core::ProviderName::Openai).await;
    contract_validate_known_model_ok(&provider, "mock-model");
    contract_validate_unknown_model_errors_pre_flight(&provider, "not-a-real-model");

    let request = ProviderRequest::new("contract test prompt", "mock-model");
    let response = contract_run_response_carries_provider_identity(
        &provider,
        request,
        anseo_core::ProviderName::Openai,
    )
    .await;
    assert_eq!(response.message_text, "contract test response");
}

#[tokio::test]
async fn mock_provider_satisfies_contract_for_anthropic_identity() {
    // Same contract, different identity — confirms `MockProvider::name()`
    // tracks the constructor argument rather than being hard-coded.
    let provider = MockProvider::new(anseo_core::ProviderName::Anthropic)
        .accept_model("mock-model")
        .queue_response("hello from contract");

    contract_name_is_stable(&provider, anseo_core::ProviderName::Anthropic).await;
    contract_validate_known_model_ok(&provider, "mock-model");

    let request = ProviderRequest::new("anything", "mock-model");
    contract_run_response_carries_provider_identity(
        &provider,
        request,
        anseo_core::ProviderName::Anthropic,
    )
    .await;
}

#[tokio::test]
async fn mock_provider_maps_rate_limit_failure_to_closed_taxonomy() {
    let provider = MockProvider::new(anseo_core::ProviderName::Openai)
        .accept_model("mock-model")
        .queue_failure(ProviderError::rate_limited("429 from upstream"));

    let request = ProviderRequest::new("anything", "mock-model");
    contract_run_failure_maps_to_closed_taxonomy(
        &provider,
        request,
        ProviderErrorKind::ProviderRateLimited,
    )
    .await;
}

#[tokio::test]
async fn mock_provider_maps_timeout_failure_to_closed_taxonomy() {
    let provider = MockProvider::new(anseo_core::ProviderName::Openai)
        .accept_model("mock-model")
        .queue_failure(ProviderError::timeout("provider timeout"));

    let request = ProviderRequest::new("anything", "mock-model");
    contract_run_failure_maps_to_closed_taxonomy(
        &provider,
        request,
        ProviderErrorKind::ProviderTimeout,
    )
    .await;
}

#[tokio::test]
async fn mock_provider_maps_5xx_failure_to_closed_taxonomy() {
    let provider = MockProvider::new(anseo_core::ProviderName::Anthropic)
        .accept_model("mock-model")
        .queue_failure(ProviderError::five_xx("503 service unavailable"));

    let request = ProviderRequest::new("anything", "mock-model");
    contract_run_failure_maps_to_closed_taxonomy(
        &provider,
        request,
        ProviderErrorKind::Provider5xx,
    )
    .await;
}

#[tokio::test]
async fn mock_provider_maps_unauthorized_failure_to_closed_taxonomy() {
    let provider = MockProvider::new(anseo_core::ProviderName::Openai)
        .accept_model("mock-model")
        .queue_failure(ProviderError::unauthorized("401"));

    let request = ProviderRequest::new("anything", "mock-model");
    contract_run_failure_maps_to_closed_taxonomy(
        &provider,
        request,
        ProviderErrorKind::ProviderUnauthorized,
    )
    .await;
}

#[tokio::test]
async fn mock_provider_maps_invalid_response_failure_to_closed_taxonomy() {
    let provider = MockProvider::new(anseo_core::ProviderName::Anthropic)
        .accept_model("mock-model")
        .queue_failure(ProviderError::invalid_response("malformed JSON"));

    let request = ProviderRequest::new("anything", "mock-model");
    contract_run_failure_maps_to_closed_taxonomy(
        &provider,
        request,
        ProviderErrorKind::ProviderInvalidResponse,
    )
    .await;
}

#[tokio::test]
async fn mock_provider_maps_network_failure_to_closed_taxonomy() {
    let provider = MockProvider::new(anseo_core::ProviderName::Openai)
        .accept_model("mock-model")
        .queue_failure(ProviderError::network("connection refused"));

    let request = ProviderRequest::new("anything", "mock-model");
    contract_run_failure_maps_to_closed_taxonomy(
        &provider,
        request,
        ProviderErrorKind::NetworkError,
    )
    .await;
}
