"""opengeo-observe — thin instrumentation SDK for the OpenGEO Run-Ingestion API.

The OpenTelemetry pattern, minus the ceremony: you already ran a prompt against
an LLM provider *outside* OpenGEO. This SDK lets you POST that run to
``POST /v1/ingest/run`` in one call, so it flows through the same
extraction -> redaction -> benchmark-contribution path as a native run.

No third-party dependencies — it uses the standard-library ``urllib``.

Example
-------
::

    from anseo_observe import AnseoObserver

    observer = AnseoObserver(
        base_url="https://anseo.internal",
        api_key=os.environ["ANSEO_API_KEY"],
        project="Sunski",
    )

    result = observer.observe_run(
        prompt_slug="best-polarized-sunglasses",
        provider="openai",
        model="gpt-4o-2024-08-06",
        response_text=completion.choices[0].message.content,
    )
    print(result.contribution)  # e.g. {"status": "sealed"}
"""

from .client import (
    ObserveRunResult,
    OpenGeoApiError,
    AnseoObserver,
    observe_run,
)

__all__ = [
    "AnseoObserver",
    "ObserveRunResult",
    "OpenGeoApiError",
    "observe_run",
]

__version__ = "0.1.0"
