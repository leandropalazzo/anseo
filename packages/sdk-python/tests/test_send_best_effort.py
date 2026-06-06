"""Best-effort, at-most-once delivery semantics for ``AnseoObserver.send``."""

from __future__ import annotations

import json
import logging

from anseo_observe import AnseoObserver


class RecordingTransport:
    def __init__(self, status=200, body=None, raises=None):
        self.status = status
        self.body = body or {
            "run_id": "run_1",
            "project_id": "p",
            "prompt_slug": "p",
            "provider": "openai",
            "observed_at": "2026-06-04T12:00:00Z",
            "contribution": {"status": "sealed"},
        }
        self.raises = raises
        self.calls = 0

    def __call__(self, url, method, headers, body):
        self.calls += 1
        if self.raises is not None:
            raise self.raises
        return self.status, json.dumps(self.body)


def test_send_happy_path_returns_result():
    transport = RecordingTransport()
    obs = AnseoObserver(base_url="https://x", api_key="k", transport=transport)
    result = obs.send(prompt_slug="p", provider="openai", model="m")
    assert result is not None
    assert result.run_id == "run_1"


def test_send_swallows_network_error_returns_none_and_logs_debug(caplog):
    transport = RecordingTransport(raises=TimeoutError("connect timed out"))
    obs = AnseoObserver(base_url="https://x", api_key="k", transport=transport)
    with caplog.at_level(logging.DEBUG, logger="anseo"):
        result = obs.send(prompt_slug="p", provider="openai", model="m")
    assert result is None
    assert any("discarded" in r.message for r in caplog.records)


def test_send_does_not_retry_on_5xx_at_most_once():
    transport = RecordingTransport(status=503, body={"error": "unavailable"})
    obs = AnseoObserver(base_url="https://x", api_key="k", transport=transport)
    result = obs.send(prompt_slug="p", provider="openai", model="m")
    assert result is None
    # At-most-once: exactly one attempt, never a retry on 5xx.
    assert transport.calls == 1


def test_send_logs_401_at_warning(caplog):
    transport = RecordingTransport(status=401, body={"error": "unauthorized"})
    obs = AnseoObserver(base_url="https://x", api_key="bad", transport=transport)
    with caplog.at_level(logging.WARNING, logger="anseo"):
        result = obs.send(prompt_slug="p", provider="openai", model="m")
    assert result is None
    warns = [r for r in caplog.records if r.levelno == logging.WARNING]
    assert warns and "API key" in warns[0].message
