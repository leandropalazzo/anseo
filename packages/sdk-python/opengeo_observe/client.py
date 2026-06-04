"""Standard-library-only client for ``POST /v1/ingest/run``."""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Callable, Mapping, Optional, Sequence

_INGEST_PATH = "/v1/ingest/run"
_DEFAULT_TIMEOUT = 30.0

# A pluggable transport: takes (url, method, headers, body) and returns
# (status_code, response_text). The default uses urllib; tests inject a fake.
Transport = Callable[[str, str, Mapping[str, str], bytes], "tuple[int, str]"]


class OpenGeoApiError(Exception):
    """Raised when the API returns a non-2xx response."""

    def __init__(self, status: int, code: Optional[str], message: str) -> None:
        super().__init__(message)
        self.status = status
        self.code = code
        self.message = message


@dataclass(frozen=True)
class ObserveRunResult:
    """Parsed ``IngestRunResponse``."""

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
    def from_json(cls, data: Mapping[str, Any]) -> "ObserveRunResult":
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
    ) -> "tuple[int, str]":
        req = urllib.request.Request(
            url, data=body, headers=dict(headers), method=method
        )
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.status, resp.read().decode("utf-8")
        except urllib.error.HTTPError as exc:  # non-2xx
            text = exc.read().decode("utf-8") if exc.fp is not None else ""
            return exc.code, text

    return transport


def _to_iso(value: Any) -> Any:
    if isinstance(value, datetime):
        return value.isoformat()
    return value


class OpenGeoObserver:
    """Thin client around ``POST /v1/ingest/run``."""

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
            raise ValueError("`base_url` is required")
        if not api_key:
            raise ValueError("`api_key` is required")
        # Normalize trailing slashes so URL joining is unambiguous.
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.project = project
        self._transport = transport or _urllib_transport(timeout)

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
    ) -> ObserveRunResult:
        """Record one externally-executed run.

        Returns the parsed :class:`ObserveRunResult`; raises
        :class:`OpenGeoApiError` on a non-2xx response.
        """
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

        headers = {
            "content-type": "application/json",
            "x-opengeo-api-key": self.api_key,
        }
        if self.project:
            headers["x-opengeo-project"] = self.project

        url = self.base_url + _INGEST_PATH
        encoded = json.dumps(body).encode("utf-8")
        status, text = self._transport(url, "POST", headers, encoded)

        try:
            parsed = json.loads(text) if text else None
        except json.JSONDecodeError:
            parsed = None

        if not 200 <= status < 300:
            code = parsed.get("error") if isinstance(parsed, dict) else None
            message = (
                parsed.get("message")
                if isinstance(parsed, dict) and parsed.get("message")
                else f"OpenGEO ingest failed: HTTP {status}"
            )
            raise OpenGeoApiError(status, code, message)

        if not isinstance(parsed, dict):
            raise OpenGeoApiError(
                status, None, "OpenGEO ingest returned a non-object body"
            )
        return ObserveRunResult.from_json(parsed)


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
    timeout: float = _DEFAULT_TIMEOUT,
    transport: Optional[Transport] = None,
) -> ObserveRunResult:
    """One-shot convenience: construct an observer and record a single run.

    Prefer reusing an :class:`OpenGeoObserver` when sending many runs.
    """
    observer = OpenGeoObserver(
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
    )
