"""The ``observe`` instrumentation surface (Story 40.2).

``observe`` wraps an existing LLM call so its run is shipped to Anseo without
changing your inference logic. It works two ways from one object:

- **as a context manager** — you run the call yourself, then attach the raw
  response; provider/model are auto-detected from the response object::

      obs = AnseoObserver(base_url=..., api_key=..., project="Sunski")
      with observe(obs, prompt_slug="best-sunglasses") as run:
          resp = client.chat.completions.create(...)
          run.capture(resp)              # auto-detects provider + model
      # run shipped best-effort on a clean exit

- **as a decorator** — the wrapped function's return value is treated as the
  raw response::

      @observe(obs, prompt_slug="best-sunglasses")
      def ask() -> Any:
          return client.chat.completions.create(...)

Delivery is best-effort and at-most-once (see :mod:`anseo_observe.client`):
observability never raises into, or retries inside, the host application. If
the wrapped call itself raises, **nothing is sent** (there is no run to record)
and the exception propagates unchanged.

This module never patches or monkeypatches the OpenAI/Anthropic SDKs; it only
reads documented attributes off the response object you hand it.
"""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from types import TracebackType
from typing import Any, Callable, Literal, Optional, Sequence, TypeVar, cast

from .client import AnseoObserver, _detect_provider_model

logger = logging.getLogger("anseo")

F = TypeVar("F", bound=Callable[..., Any])


class _ObserveRun:
    """The handle yielded by :class:`observe` when used as a context manager.

    Collect the call's output via :meth:`capture` (auto-detect) or by setting
    the explicit fields; the run is shipped, best-effort, on a clean exit.
    """

    def __init__(
        self,
        observer: AnseoObserver,
        *,
        prompt_slug: str,
        provider: Optional[str],
        model: Optional[str],
        observed_rank: Optional[int],
        citation_domains: Optional[Sequence[str]],
        contribute: Optional[bool],
    ) -> None:
        self._observer = observer
        self._prompt_slug = prompt_slug
        self.provider = provider
        self.model = model
        self.response_text: Optional[str] = None
        self.observed_rank = observed_rank
        self.citation_domains = citation_domains
        self.contribute = contribute
        self._captured = False

    def capture(
        self,
        raw_response: Any,
        *,
        provider: Optional[str] = None,
        model: Optional[str] = None,
    ) -> _ObserveRun:
        """Auto-detect provider/model and extract text from ``raw_response``.

        Explicit ``provider``/``model`` override auto-detection. Calling this
        marks the run for delivery on context exit.
        """
        det_provider, det_model = _detect_provider_model(raw_response)
        self.provider = provider or self.provider or det_provider
        self.model = model or self.model or det_model
        text = _extract_text(raw_response)
        if text is not None:
            self.response_text = text
        self._captured = True
        return self

    def __enter__(self) -> _ObserveRun:
        return self

    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc: Optional[BaseException],
        tb: Optional[TracebackType],
    ) -> Literal[False]:
        # If the wrapped call raised, there is no run to record.
        if exc_type is not None:
            return False
        self._ship()
        return False

    def _ship(self) -> None:
        if not self._captured:
            logger.debug(
                "anseo: observe() block for %r exited without capture(); "
                "nothing sent",
                self._prompt_slug,
            )
            return
        # provider defaults to the server-validated sentinel "unknown" when it
        # could not be detected and the caller did not supply it.
        provider = self.provider or "unknown"
        if not self.model:
            logger.debug(
                "anseo: could not determine model for %r; skipping send "
                "(supply model= explicitly)",
                self._prompt_slug,
            )
            return
        self._observer.send(
            prompt_slug=self._prompt_slug,
            provider=provider,
            model=self.model,
            response_text=self.response_text,
            citation_domains=self.citation_domains,
            observed_rank=self.observed_rank,
            observed_at=datetime.now(timezone.utc),
            contribute=self.contribute,
        )


class observe:  # noqa: N801 — public API spelled lowercase by design
    """Instrument an LLM call. Usable as a context manager *or* a decorator.

    :param observer: the configured :class:`AnseoObserver` to send through.
    :param prompt_slug: the declared prompt slug for this run.
    :param provider: optional explicit provider; overrides auto-detect.
    :param model: optional explicit model; overrides auto-detect.
    :param observed_rank: optional pre-computed brand rank for the run.
    :param citation_domains: optional pre-extracted citation domains.
    :param contribute: opt this run into Anseo's benchmark contribution path.
    """

    def __init__(
        self,
        observer: AnseoObserver,
        *,
        prompt_slug: str,
        provider: Optional[str] = None,
        model: Optional[str] = None,
        observed_rank: Optional[int] = None,
        citation_domains: Optional[Sequence[str]] = None,
        contribute: Optional[bool] = None,
    ) -> None:
        self._observer = observer
        self._prompt_slug = prompt_slug
        self._provider = provider
        self._model = model
        self._observed_rank = observed_rank
        self._citation_domains = citation_domains
        self._contribute = contribute
        self._run: Optional[_ObserveRun] = None

    def _new_run(self) -> _ObserveRun:
        return _ObserveRun(
            self._observer,
            prompt_slug=self._prompt_slug,
            provider=self._provider,
            model=self._model,
            observed_rank=self._observed_rank,
            citation_domains=self._citation_domains,
            contribute=self._contribute,
        )

    # --- context-manager protocol -----------------------------------------
    def __enter__(self) -> _ObserveRun:
        self._run = self._new_run()
        return self._run

    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc: Optional[BaseException],
        tb: Optional[TracebackType],
    ) -> Literal[False]:
        run = self._run
        self._run = None
        if run is None:
            return False
        return run.__exit__(exc_type, exc, tb)

    # --- decorator protocol -----------------------------------------------
    def __call__(self, func: F) -> F:
        import functools
        import inspect

        # Async coroutine functions (the common case for real LLM SDKs, e.g.
        # ``await client.chat.completions.create(...)``) need an async wrapper
        # that ``await``s the wrapped call. The sync path is left untouched.
        if inspect.iscoroutinefunction(func):

            @functools.wraps(func)
            async def async_wrapper(*args: Any, **kwargs: Any) -> Any:
                run = self._new_run()
                result = await func(*args, **kwargs)  # raises => nothing sent
                run.capture(result)
                run._ship()
                return result

            return cast(F, async_wrapper)

        @functools.wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            run = self._new_run()
            result = func(*args, **kwargs)  # raises => nothing recorded
            run.capture(result)
            run._ship()
            return result

        return cast(F, wrapper)


def _extract_text(raw_response: Any) -> Optional[str]:
    """Pull assistant text out of a known OpenAI/Anthropic response shape.

    Documented attribute paths only — no SDK import or patching:

    - **OpenAI chat** (``openai>=1.0``):
      ``response.choices[0].message.content`` (str).
    - **OpenAI Responses API**: ``response.output_text`` (str).
    - **Anthropic Messages** (``anthropic>=0.21``): ``response.content`` is a
      list of blocks; concatenate the ``.text`` of each text block.
    - plain ``str`` is returned as-is.

    Returns ``None`` when no text can be extracted (the caller may still set
    :attr:`_ObserveRun.response_text` manually).
    """
    if raw_response is None:
        return None
    if isinstance(raw_response, str):
        return raw_response

    # OpenAI Responses API convenience accessor.
    output_text = getattr(raw_response, "output_text", None)
    if isinstance(output_text, str) and output_text:
        return output_text

    # OpenAI chat.completions.
    choices = getattr(raw_response, "choices", None)
    if choices:
        try:
            content = choices[0].message.content
            if isinstance(content, str):
                return content
        except (AttributeError, IndexError, TypeError):
            pass

    # Anthropic Messages: content is a list of typed blocks.
    content = getattr(raw_response, "content", None)
    if isinstance(content, list):
        parts: list[str] = []
        for block in content:
            text = getattr(block, "text", None)
            if isinstance(text, str):
                parts.append(text)
        if parts:
            return "".join(parts)

    return None
