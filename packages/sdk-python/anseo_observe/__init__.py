"""anseo-observe — the instrumentation SDK for the Anseo Run-Ingestion API.

The OpenTelemetry pattern, minus the ceremony: you already ran a prompt against
an LLM provider *outside* Anseo. This SDK ships that run to
``POST /v1/ingest/run`` so it flows through the same
extraction -> redaction -> benchmark-contribution path as a native run.

No third-party dependencies — it uses the standard-library ``urllib``.

Quickstart (best-effort, never interrupts your app)::

    from anseo_observe import AnseoObserver, observe

    obs = AnseoObserver(
        base_url="https://anseo.internal",
        api_key=os.environ["ANSEO_API_KEY"],
        project="Sunski",
    )

    with observe(obs, prompt_slug="best-polarized-sunglasses") as run:
        resp = openai_client.chat.completions.create(...)
        run.capture(resp)   # auto-detects provider + model

See :mod:`anseo_observe.observe` for the decorator form and
:mod:`anseo_observe.client` for the strict ``observe_run`` API.
"""

from .client import (
    AnseoApiError,
    AnseoConfigError,
    AnseoObserver,
    ObserveRunResult,
    OpenGeoApiError,
    observe_run,
)
from .observe import observe

__all__ = [
    "AnseoObserver",
    "AnseoApiError",
    "AnseoConfigError",
    "ObserveRunResult",
    "OpenGeoApiError",
    "observe",
    "observe_run",
]

# Resolve the runtime version from the INSTALLED package metadata so it can
# never drift from the wheel the release train publishes (the release pins
# [project].version in pyproject.toml; importlib reads that same value at
# runtime). Falls back to the in-tree literal for editable/uninstalled dev use.
try:  # pragma: no cover - trivial metadata lookup
    from importlib.metadata import PackageNotFoundError, version as _pkg_version

    try:
        __version__ = _pkg_version("anseo-observe")
    except PackageNotFoundError:
        __version__ = "0.1.0"
except ImportError:  # pragma: no cover - importlib.metadata always present on 3.8+
    __version__ = "0.1.0"
