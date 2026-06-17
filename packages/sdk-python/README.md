# anseo-observe (Python)

Instrumentation SDK that ships **externally-executed** LLM runs to Anseo's
Run-Ingestion API (`POST /v1/ingest/run`). The OpenTelemetry pattern, minus the
ceremony: you already ran a prompt against a provider outside Anseo — this posts
that run so it flows through the same extraction → redaction →
benchmark-contribution path as a native run.

- **Zero runtime dependencies** — standard-library `urllib` only.
- **Best-effort, at-most-once** — never crashes your app, never retries, never
  double-records (see [`docs/sdk-spec.md`](../../docs/sdk-spec.md)).
- Sends the API key as `X-Anseo-API-Key` and scopes the run with
  `X-Anseo-Project` (brand name).

## Quickstart

```python
from anseo_observe import AnseoObserver, observe

obs = AnseoObserver(base_url="https://anseo.internal",
                    api_key=os.environ["ANSEO_API_KEY"], project="Sunski")
with observe(obs, prompt_slug="best-polarized-sunglasses") as run:
    resp = openai_client.chat.completions.create(...); run.capture(resp)
```

`run.capture(resp)` auto-detects `provider` + `model` and extracts the response
text from the OpenAI/Anthropic response object (no monkeypatching). Delivery is
best-effort: a network/server error is logged on the `anseo` logger and
swallowed — your app is never interrupted.

## Decorator form

```python
@observe(obs, prompt_slug="best-polarized-sunglasses")
def ask():
    return openai_client.chat.completions.create(...)  # return value is the run
```

## Strict / manual client

When you want to *know* each contribution status (e.g. a backfill), use the
strict surface — it returns the parsed result and raises on a non-2xx:

```python
result = obs.observe_run(
    prompt_slug="best-polarized-sunglasses",
    provider="openai",
    model="gpt-4o-2024-08-06",
    response_text=completion.choices[0].message.content,
    # optional: citation_domains=["sunski.com"], observed_rank=1, observed_at=...
    # optional: contribute=True  # requires project KEK; consent controls sealing
)
print(result.run_id, result.contribution["status"])
# status ∈ {"sealed", "skipped_not_opted_in", "redaction_rejected"}
# missing KEK is currently a 403 request error, not an accepted result
```

Construction raises `AnseoConfigError` when `base_url`/`api_key` is missing;
`observe_run` raises `AnseoApiError` (with `.status`, `.code`) on a non-2xx.

## Debug logging

```python
import logging; logging.getLogger("anseo").setLevel(logging.DEBUG)
```

`401` (bad API key) logs at WARNING; all other failures at DEBUG.

## Develop / test

```bash
python -m venv .venv && . .venv/bin/activate
pip install -e ".[dev]"
pytest && ruff check . && mypy
```
