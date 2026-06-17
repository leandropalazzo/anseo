"""Unit tests for the strict ``AnseoObserver.observe_run`` surface.

The HTTP transport is always mocked — no test touches the network.
"""

from __future__ import annotations

import json
from datetime import datetime, timezone

import pytest

from anseo_observe import (
    AnseoApiError,
    AnseoConfigError,
    AnseoObserver,
    ObserveRunResult,
    observe_run,
)

OK_BODY = {
    "run_id": "run_123",
    "project_id": "proj_abc",
    "prompt_slug": "best-polarized-sunglasses",
    "provider": "openai",
    "observed_at": "2026-06-04T12:00:00Z",
    "contribution": {"status": "sealed"},
}


class RecordingTransport:
    """Captures the request and returns a canned (status, text)."""

    def __init__(self, status=200, body=None):
        self.status = status
        self.body = OK_BODY if body is None else body
        self.calls = []

    def __call__(self, url, method, headers, body):
        self.calls.append(
            {
                "url": url,
                "method": method,
                "headers": dict(headers),
                "body": json.loads(body.decode("utf-8")),
            }
        )
        return self.status, json.dumps(self.body)


def test_posts_to_ingest_run_with_canonical_headers_and_snake_case_body():
    transport = RecordingTransport()
    observer = AnseoObserver(
        base_url="https://anseo.internal/",  # trailing slash must normalize
        api_key="key-xyz",
        project="Sunski",
        transport=transport,
    )

    result = observer.observe_run(
        prompt_slug="best-polarized-sunglasses",
        provider="openai",
        model="gpt-4o-2024-08-06",
        response_text="Try Sunski, see https://sunski.com",
        observed_rank=1,
        observed_at=datetime(2026, 6, 4, 12, 0, 0, tzinfo=timezone.utc),
        contribute=True,
    )

    assert isinstance(result, ObserveRunResult)
    assert result.run_id == "run_123"
    assert result.contribution == {"status": "sealed"}

    assert len(transport.calls) == 1
    call = transport.calls[0]
    assert call["url"] == "https://anseo.internal/v1/ingest/run"
    assert call["method"] == "POST"
    # Canonical post-rename headers.
    assert call["headers"]["x-anseo-api-key"] == "key-xyz"
    assert call["headers"]["x-anseo-project"] == "Sunski"
    assert call["headers"]["content-type"] == "application/json"
    assert call["body"] == {
        "prompt_slug": "best-polarized-sunglasses",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "response_text": "Try Sunski, see https://sunski.com",
        "observed_rank": 1,
        "observed_at": "2026-06-04T12:00:00+00:00",
        "contribute": True,
    }


def test_omits_project_header_and_optional_fields_when_unset():
    transport = RecordingTransport()
    observer = AnseoObserver(
        base_url="https://anseo.internal",
        api_key="key-xyz",
        transport=transport,
    )

    observer.observe_run(
        prompt_slug="best-polarized-sunglasses",
        provider="openai",
        model="gpt-4o-2024-08-06",
    )

    call = transport.calls[0]
    assert "x-anseo-project" not in call["headers"]
    assert call["body"] == {
        "prompt_slug": "best-polarized-sunglasses",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
    }


def test_surfaces_kek_missing_status():
    body = {**OK_BODY, "contribution": {"status": "kek_missing"}}
    observer = AnseoObserver(
        base_url="https://x",
        api_key="k",
        transport=RecordingTransport(body=body),
    )
    result = observer.observe_run(prompt_slug="p", provider="openai", model="m")
    assert result.contribution == {"status": "kek_missing"}


def test_unsupported_provider_is_passed_through_for_server_validation():
    transport = RecordingTransport()
    observer = AnseoObserver(base_url="https://x", api_key="k", transport=transport)
    observer.observe_run(prompt_slug="p", provider="unknown", model="m")
    assert transport.calls[0]["body"]["provider"] == "unknown"


def test_observe_run_raises_on_non_2xx_with_status_and_code():
    transport = RecordingTransport(
        status=404,
        body={"error": "prompt_not_found", "message": "prompt `p` is not declared"},
    )
    observer = AnseoObserver(base_url="https://x", api_key="k", transport=transport)

    with pytest.raises(AnseoApiError) as excinfo:
        observer.observe_run(prompt_slug="p", provider="openai", model="m")

    assert excinfo.value.status == 404
    assert excinfo.value.code == "prompt_not_found"
    assert "not declared" in excinfo.value.message


def test_missing_api_key_raises_config_error_at_construction_not_call_time():
    with pytest.raises(AnseoConfigError):
        AnseoObserver(base_url="https://x", api_key="")
    with pytest.raises(AnseoConfigError):
        AnseoObserver(base_url="", api_key="k")


def test_one_shot_observe_run_helper():
    transport = RecordingTransport()
    result = observe_run(
        base_url="https://anseo.internal",
        api_key="k",
        prompt_slug="p",
        provider="openai",
        model="m",
        transport=transport,
    )
    assert result.run_id == "run_123"
    assert len(transport.calls) == 1
