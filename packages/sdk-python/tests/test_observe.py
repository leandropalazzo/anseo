"""Tests for the ``observe`` decorator/context-manager and auto-detect.

Provider/model auto-detection inspects *fake* response objects mimicking the
documented OpenAI/Anthropic shapes — the real SDKs are never imported.
"""

from __future__ import annotations

import json

from anseo_observe import AnseoObserver, observe


class RecordingTransport:
    def __init__(self):
        self.calls = []

    def __call__(self, url, method, headers, body):
        self.calls.append(json.loads(body.decode("utf-8")))
        return 200, json.dumps(
            {
                "run_id": "run_1",
                "project_id": "p",
                "prompt_slug": "p",
                "provider": "openai",
                "observed_at": "2026-06-04T12:00:00Z",
                "contribution": {"status": "sealed"},
            }
        )


def _observer(transport):
    return AnseoObserver(
        base_url="https://x", api_key="k", project="Sunski", transport=transport
    )


# --- fake response shapes (documented attribute paths) --------------------
class _Msg:
    def __init__(self, content):
        self.content = content


class _Choice:
    def __init__(self, content):
        self.message = _Msg(content)


class FakeOpenAIResponse:
    object = "chat.completion"

    def __init__(self, model, content):
        self.model = model
        self.choices = [_Choice(content)]


class _Block:
    def __init__(self, text):
        self.type = "text"
        self.text = text


class FakeAnthropicResponse:
    type = "message"

    def __init__(self, model, text):
        self.model = model
        self.content = [_Block(text)]


def test_context_manager_autodetects_openai_and_ships():
    transport = RecordingTransport()
    obs = _observer(transport)

    with observe(obs, prompt_slug="best-sunglasses") as run:
        resp = FakeOpenAIResponse("gpt-4o-2024-08-06", "Try Sunski https://sunski.com")
        run.capture(resp)

    assert len(transport.calls) == 1
    body = transport.calls[0]
    assert body["prompt_slug"] == "best-sunglasses"
    assert body["provider"] == "openai"
    assert body["model"] == "gpt-4o-2024-08-06"
    assert body["response_text"] == "Try Sunski https://sunski.com"
    assert "observed_at" in body


def test_context_manager_autodetects_anthropic():
    transport = RecordingTransport()
    with observe(_observer(transport), prompt_slug="p") as run:
        run.capture(FakeAnthropicResponse("claude-3-5-sonnet-20241022", "Hello"))
    body = transport.calls[0]
    assert body["provider"] == "anthropic"
    assert body["model"] == "claude-3-5-sonnet-20241022"
    assert body["response_text"] == "Hello"


def test_decorator_ships_return_value():
    transport = RecordingTransport()
    obs = _observer(transport)

    @observe(obs, prompt_slug="p")
    def ask():
        return FakeOpenAIResponse("gpt-4o-mini", "answer")

    result = ask()
    assert isinstance(result, FakeOpenAIResponse)
    assert len(transport.calls) == 1
    assert transport.calls[0]["model"] == "gpt-4o-mini"


def test_explicit_provider_model_override_autodetect():
    transport = RecordingTransport()
    with observe(
        _observer(transport),
        prompt_slug="p",
        provider="azure-openai",
        model="custom",
    ) as run:
        run.capture("just a string")
    body = transport.calls[0]
    assert body["provider"] == "azure-openai"
    assert body["model"] == "custom"
    assert body["response_text"] == "just a string"


def test_nothing_shipped_when_wrapped_call_raises():
    transport = RecordingTransport()
    try:
        with observe(_observer(transport), prompt_slug="p") as run:
            raise RuntimeError("LLM call blew up")
            run.capture("never")  # noqa: F811 — unreachable by design
    except RuntimeError:
        pass
    assert transport.calls == []


def test_no_capture_means_no_send():
    transport = RecordingTransport()
    with observe(_observer(transport), prompt_slug="p"):
        pass  # never captured a response
    assert transport.calls == []


def test_undetectable_model_skips_send():
    transport = RecordingTransport()

    class Opaque:
        pass

    with observe(_observer(transport), prompt_slug="p") as run:
        run.capture(Opaque())
    # No model could be detected and none supplied → nothing sent.
    assert transport.calls == []
