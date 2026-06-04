"""Unit tests for opengeo_observe — the HTTP transport is mocked."""

from __future__ import annotations

import json
from datetime import datetime, timezone

import pytest

from opengeo_observe import ObserveRunResult, OpenGeoApiError, OpenGeoObserver, observe_run

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


def test_posts_to_ingest_run_with_headers_and_snake_case_body():
    transport = RecordingTransport()
    observer = OpenGeoObserver(
        base_url="https://opengeo.internal/",  # trailing slash must normalize
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
    )

    assert isinstance(result, ObserveRunResult)
    assert result.run_id == "run_123"
    assert result.contribution == {"status": "sealed"}

    assert len(transport.calls) == 1
    call = transport.calls[0]
    assert call["url"] == "https://opengeo.internal/v1/ingest/run"
    assert call["method"] == "POST"
    assert call["headers"]["x-opengeo-api-key"] == "key-xyz"
    assert call["headers"]["x-opengeo-project"] == "Sunski"
    assert call["headers"]["content-type"] == "application/json"
    assert call["body"] == {
        "prompt_slug": "best-polarized-sunglasses",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
        "response_text": "Try Sunski, see https://sunski.com",
        "observed_rank": 1,
        "observed_at": "2026-06-04T12:00:00+00:00",
    }


def test_omits_project_header_and_optional_fields_when_unset():
    transport = RecordingTransport()
    observer = OpenGeoObserver(
        base_url="https://opengeo.internal",
        api_key="key-xyz",
        transport=transport,
    )

    observer.observe_run(
        prompt_slug="best-polarized-sunglasses",
        provider="openai",
        model="gpt-4o-2024-08-06",
    )

    call = transport.calls[0]
    assert "x-opengeo-project" not in call["headers"]
    assert call["body"] == {
        "prompt_slug": "best-polarized-sunglasses",
        "provider": "openai",
        "model": "gpt-4o-2024-08-06",
    }


def test_surfaces_kek_missing_status():
    body = {**OK_BODY, "contribution": {"status": "kek_missing"}}
    observer = OpenGeoObserver(
        base_url="https://x",
        api_key="k",
        transport=RecordingTransport(body=body),
    )
    result = observer.observe_run(prompt_slug="p", provider="openai", model="m")
    assert result.contribution == {"status": "kek_missing"}


def test_raises_on_non_2xx_with_status_and_code():
    transport = RecordingTransport(
        status=404,
        body={"error": "prompt_not_found", "message": "prompt `p` is not declared"},
    )
    observer = OpenGeoObserver(base_url="https://x", api_key="k", transport=transport)

    with pytest.raises(OpenGeoApiError) as excinfo:
        observer.observe_run(prompt_slug="p", provider="openai", model="m")

    assert excinfo.value.status == 404
    assert excinfo.value.code == "prompt_not_found"
    assert "not declared" in excinfo.value.message


def test_requires_base_url_and_api_key():
    with pytest.raises(ValueError):
        OpenGeoObserver(base_url="", api_key="k")
    with pytest.raises(ValueError):
        OpenGeoObserver(base_url="https://x", api_key="")


def test_one_shot_observe_run_helper():
    transport = RecordingTransport()
    result = observe_run(
        base_url="https://opengeo.internal",
        api_key="k",
        prompt_slug="p",
        provider="openai",
        model="m",
        transport=transport,
    )
    assert result.run_id == "run_123"
    assert len(transport.calls) == 1
