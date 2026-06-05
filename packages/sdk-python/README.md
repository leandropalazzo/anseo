# opengeo-observe (Python)

Thin instrumentation SDK to send **externally-executed** LLM runs to Anseo's
Run-Ingestion API (`POST /v1/ingest/run`). The OpenTelemetry pattern, minus the
ceremony: you already ran a prompt against a provider outside Anseo — this
posts that run so it flows through the same extraction → redaction →
benchmark-contribution path as a native run.

- **Zero runtime dependencies** — standard-library `urllib` only.
- Sends the API key as `X-OpenGEO-API-Key` and scopes the run with
  `X-OpenGEO-Project` (brand name).

## Install

```bash
pip install opengeo-observe
```

## One-liner integration

```python
from opengeo_observe import observe_run

observe_run(
    base_url="https://opengeo.internal",
    api_key=os.environ["OPENGEO_API_KEY"],
    project="Sunski",
    prompt_slug="best-polarized-sunglasses",
    provider="openai",
    model="gpt-4o-2024-08-06",
    response_text=completion_text,
)
```

## Reusable client

```python
import os
from opengeo_observe import OpenGeoObserver

observer = OpenGeoObserver(
    base_url="https://opengeo.internal",
    api_key=os.environ["OPENGEO_API_KEY"],
    project="Sunski",  # omit for single-project deployments
)

result = observer.observe_run(
    prompt_slug="best-polarized-sunglasses",
    provider="openai",
    model="gpt-4o-2024-08-06",
    response_text=completion.choices[0].message.content,
    # optional:
    # citation_domains=["sunski.com"],
    # observed_rank=1,
    # observed_at=datetime.now(timezone.utc),
)

# result.contribution tells you whether benchmark data was sealed:
# {"status": "sealed"} | {"status": "skipped_not_opted_in"}
# | {"status": "kek_missing"} | {"status": "redaction_rejected", "reason": ...}
print(result.run_id, result.contribution["status"])
```

Non-2xx responses raise `OpenGeoApiError` with `.status` and `.code`.

## Develop / test

```bash
python -m venv .venv && . .venv/bin/activate
pip install -e ".[test]"
pytest
```
