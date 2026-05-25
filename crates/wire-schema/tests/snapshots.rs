use insta::assert_json_snapshot;
use opengeo_core::{ProviderErrorKind, RequestId};
use opengeo_wire_schema::{ApiError, ErrorEnvelope};
use ulid::Ulid;

fn fixed_request_id() -> RequestId {
    // Deterministic ULID for snapshot stability.
    RequestId::from_ulid(Ulid::from_string("01J0000000000000000000000A").unwrap())
}

#[test]
fn error_envelope_snapshot_provider_rate_limited() {
    let envelope = ErrorEnvelope {
        error: ApiError {
            kind: ProviderErrorKind::ProviderRateLimited,
            message: "OpenAI rate limit exceeded".into(),
            details: Some(serde_json::json!({"retry_after_seconds": 30})),
            request_id: fixed_request_id(),
        },
    };
    assert_json_snapshot!(envelope);
}

#[test]
fn error_envelope_snapshot_minimal() {
    let envelope = ErrorEnvelope {
        error: ApiError {
            kind: ProviderErrorKind::NetworkError,
            message: "DNS failure".into(),
            details: None,
            request_id: fixed_request_id(),
        },
    };
    assert_json_snapshot!(envelope);
}
