"""Standard-library-only client for ``POST /v1/ingest/run`` (Story 40.2).

This is the **instrumentation** client: a thin, best-effort, *at-most-once*
sender that ships externally-executed LLM runs to the Anseo Run-Ingestion API.
It is the client side of the 40.1 contract (`apps/api/src/routes/ingest.rs`).

Two delivery surfaces, by design:

- :meth:`AnseoObserver.observe_run` — **strict**. Returns the parsed
  :class:`ObserveRunResult` and raises :class:`AnseoApiError` on a non-2xx
  response. Use it for manual, synchronous control (e.g. a backfill script
  that wants to know each contribution status).

- :meth:`AnseoObserver.send` and the :func:`observe` decorator/context-manager
  — **best-effort**. Per the core spec, observability must never interrupt the
  host application: any transport or server error is logged (at DEBUG, or WARN
  for a ``401``) and swallowed. **No status is ever retried** — at-most-once
  delivery is intentional (a retry on 5xx could double-record a run the server
  already processed before timing out).

No third-party dependencies — standard-library ``urllib`` only.
"""

from __future__ import annotations

import json
import logging
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Callable, Mapping, Optional, Sequence

_INGEST_PATH = "/v1/ingest/run"
_DEFAULT_TIMEOUT = 30.0

# Canonical auth + project headers (post-rename). The API also accepts the
# legacy ``x-opengeo-*`` spellings, but new clients send the canonical names.
_API_KEY_HEADER = "x-anseo-api-key"
_PROJECT_HEADER = "x-anseo-project"

logger = logging.getLogger("anseo")

# A pluggable transport: takes (url, method, headers, body) and returns
# (status_code, response_text). The default uses urllib; tests inject a fake.
Transport = Callable[[str, str, Mapping[str, str], bytes], "tuple[int, str]"]


class AnseoConfigError(Exception):
    """Raised at construction time for an invalid SDK configuration.

    A misconfigured client (missing ``base_url`` or ``api_key``) is a
    programming error the developer must fix; it is raised eagerly at
    construction, never deferred to a call.
    """


class AnseoApiError(Exception):
    """Raised by the strict :meth:`AnseoObserver.observe_run` on a non-2xx."""

    def __init__(self, status: int, code: Optional[str], message: str) -> None:
        super().__init__(message)
        self.status = status
        self.code = code
        self.message = message


# Backwards-compatible alias for the pre-40.2 name.
OpenGeoApiError = AnseoApiError


@dataclass(frozen=True)
class ObserveRunResult:
    """Parsed ``IngestRunResponse`` from ``apps/api/src/routes/ingest.rs``."""

    run_id: str
    project_id: str
    prompt_slug: str
    provider: str
    observed_at: str
    # The internally-tagged ``ContributionStatus`` enum, e.g.
    # ``{"status": "sealed"}`` or
    # ``{"status": "redaction_rejected", "reason": "..."}``.
    contribution: Mapping[str, Any]

    @classmethod
    def from_json(cls, data: Mapping[str, Any]) -> ObserveRunResult:
        return cls(
            run_id=data["run_id"],
            project_id=data["project_id"],
            prompt_slug=data["prompt_slug"],
            provider=data["provider"],
            observed_at=data["observed_at"],
            contribution=data["contribution"],
        )


def _urllib_transport(timeout: float) -> Transport:
    def transport(
        url: str, method: str, headers: Mapping[str, str], body: bytes
    ) -> tuple[int, str]:
        req = urllib.request.Request(
            url, data=body, headers=dict(headers), method=method
        )
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.status, resp.read().decode("utf-8")
        except urllib.error.HTTPError as exc:  # non-2xx, body still readable
            text = exc.read().decode("utf-8") if exc.fp is not None else ""
            return exc.code, text

    return transport


def _to_iso(value: Any) -> Any:
    if isinstance(value, datetime):
        return value.isoformat()
    return value


def _detect_provider_model(raw_response: Any) -> tuple[Optional[str], Optional[str]]:
    """Best-effort auto-detect of ``(provider, model)`` from a raw response.

    Inspects *known attributes only* — it never imports, patches, or
    monkeypatches the OpenAI/Anthropic SDKs.

    Attribute paths relied on (and their SDK version floor):

    - **OpenAI** (``openai>=1.0``): the chat/responses object exposes
      ``response.model`` (e.g. ``"gpt-4o-2024-08-06"``) and
      ``response.object`` (e.g. ``"chat.completion"`` / ``"response"``). The
      ``object`` prefix is what identifies the provider as OpenAI.
    - **Anthropic** (``anthropic>=0.21``): the message object exposes
      ``response.model`` (e.g. ``"claude-3-5-sonnet-20241022"``) and
      ``response.type == "message"``. The ``model`` prefix (``claude-``) is
      what identifies the provider as Anthropic.

    Returns ``(None, None)`` for either field that cannot be determined; the
    caller must then supply it explicitly.
    """
    if raw_response is None:
        return None, None

    model = getattr(raw_response, "model", None)
    if not isinstance(model, str) or not model:
        model = None

    provider: Optional[str] = None
    obj = getattr(raw_response, "object", None)
    rtype = getattr(raw_response, "type", None)
    if isinstance(obj, str) and (
        obj.startswith("chat.") or obj == "response" or obj == "text_completion"
    ):
        provider = "openai"
    elif rtype == "message" or (isinstance(model, str) and model.startswith("claude")):
        provider = "anthropic"
    elif isinstance(model, str) and model.startswith("gpt"):
        provider = "openai"

    return provider, model


class AnseoObserver:
    """Thin client around ``POST /v1/ingest/run``.

    :raises AnseoConfigError: at construction if ``base_url`` or ``api_key``
        is empty.
    """

    def __init__(
        self,
        *,
        base_url: str,
        api_key: str,
        project: Optional[str] = None,
        timeout: float = _DEFAULT_TIMEOUT,
        transport: Optional[Transport] = None,
    ) -> None:
        if not base_url:
            raise AnseoConfigError("`base_url` is required")
        if not api_key:
            raise AnseoConfigError("`api_key` is required")
        # Normalize trailing slashes so URL joining is unambiguous.
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.project = project
        self._transport = transport or _urllib_transport(timeout)

    def _build(
        self,
        *,
        prompt_slug: str,
        provider: str,
        model: str,
        response_text: Optional[str],
        citation_domains: Optional[Sequence[str]],
        observed_rank: Optional[int],
        observed_at: Optional[Any],
        contribute: Optional[bool],
    ) -> tuple[str, dict[str, str], bytes]:
        body: dict[str, Any] = {
            "prompt_slug": prompt_slug,
            "provider": provider,
            "model": model,
        }
        # Omit unset optional fields so server-side defaults apply.
        if response_text is not None:
            body["response_text"] = response_text
        if citation_domains is not None:
            body["citation_domains"] = list(citation_domains)
        if observed_rank is not None:
            body["observed_rank"] = observed_rank
        if observed_at is not None:
            body["observed_at"] = _to_iso(observed_at)
        if contribute is not None:
            body["contribute"] = contribute

        headers = {
            "content-type": "application/json",
            _API_KEY_HEADER: self.api_key,
        }
        if self.project:
            headers[_PROJECT_HEADER] = self.project

        url = self.base_url + _INGEST_PATH
        return url, headers, json.dumps(body).encode("utf-8")

    def observe_run(
        self,
        *,
        prompt_slug: str,
        provider: str,
        model: str,
        response_text: Optional[str] = None,
        citation_domains: Optional[Sequence[str]] = None,
        observed_rank: Optional[int] = None,
        observed_at: Optional[Any] = None,
        contribute: Optional[bool] = None,
    ) -> ObserveRunResult:
        """Strict send: record one run, returning the parsed result.

        :raises AnseoApiError: on any non-2xx response.
        """
        url, headers, encoded = self._build(
            prompt_slug=prompt_slug,
            provider=provider,
            model=model,
            response_text=response_text,
            citation_domains=citation_domains,
            observed_rank=observed_rank,
            observed_at=observed_at,
            contribute=contribute,
        )
        status, text = self._transport(url, "POST", headers, encoded)

        try:
            parsed = json.loads(text) if text else None
        except json.JSONDecodeError:
            parsed = None

        if not 200 <= status < 300:
            code = parsed.get("error") if isinstance(parsed, dict) else None
            message: str = f"Anseo ingest failed: HTTP {status}"
            if isinstance(parsed, dict):
                server_msg = parsed.get("message")
                if isinstance(server_msg, str) and server_msg:
                    message = server_msg
            raise AnseoApiError(status, code, message)

        if not isinstance(parsed, dict):
            raise AnseoApiError(
                status, None, "Anseo ingest returned a non-object body"
            )
        return ObserveRunResult.from_json(parsed)

    def send(
        self,
        *,
        prompt_slug: str,
        provider: str,
        model: str,
        response_text: Optional[str] = None,
        citation_domains: Optional[Sequence[str]] = None,
        observed_rank: Optional[int] = None,
        observed_at: Optional[Any] = None,
        contribute: Optional[bool] = None,
    ) -> Optional[ObserveRunResult]:
        """Best-effort, at-most-once send. Never raises, never retries.

        Returns the :class:`ObserveRunResult` on success, or ``None`` when the
        run could not be delivered. Per the core spec, observability failures
        must not interrupt the host app:

        - transport/timeout/decode errors are logged at DEBUG and discarded;
        - a ``401`` (bad API key) is logged at WARN so the operator notices,
          but is still swallowed;
        - **no** status is ever retried (at-most-once delivery).

        Enable diagnostics with ``DEBUG=anseo`` / a ``logging`` handler on the
        ``"anseo"`` logger.
        """
        try:
            return self.observe_run(
                prompt_slug=prompt_slug,
                provider=provider,
                model=model,
                response_text=response_text,
                citation_domains=citation_domains,
                observed_rank=observed_rank,
                observed_at=observed_at,
                contribute=contribute,
            )
        except AnseoApiError as exc:
            if exc.status == 401:
                logger.warning(
                    "anseo: ingest rejected (401) — check your API key; "
                    "this run was NOT recorded: %s",
                    exc.message,
                )
            else:
                logger.debug(
                    "anseo: ingest returned HTTP %s (%s); run discarded: %s",
                    exc.status,
                    exc.code,
                    exc.message,
                )
            return None
        except Exception as exc:  # network/timeout/decode — best-effort
            logger.debug("anseo: ingest send failed; run discarded: %r", exc)
            return None


def observe_run(
    *,
    base_url: str,
    api_key: str,
    prompt_slug: str,
    provider: str,
    model: str,
    project: Optional[str] = None,
    response_text: Optional[str] = None,
    citation_domains: Optional[Sequence[str]] = None,
    observed_rank: Optional[int] = None,
    observed_at: Optional[Any] = None,
    contribute: Optional[bool] = None,
    timeout: float = _DEFAULT_TIMEOUT,
    transport: Optional[Transport] = None,
) -> ObserveRunResult:
    """One-shot strict convenience: construct an observer and record one run.

    Prefer reusing an :class:`AnseoObserver` when sending many runs.
    """
    observer = AnseoObserver(
        base_url=base_url,
        api_key=api_key,
        project=project,
        timeout=timeout,
        transport=transport,
    )
    return observer.observe_run(
        prompt_slug=prompt_slug,
        provider=provider,
        model=model,
        response_text=response_text,
        citation_domains=citation_domains,
        observed_rank=observed_rank,
        observed_at=observed_at,
        contribute=contribute,
    )
