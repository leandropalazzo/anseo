# Instrumentation SDK + Run Ingestion

The instrumentation SDKs let you ship **externally-executed** LLM runs into Anseo. You already ran a prompt against a provider (OpenAI, Anthropic, …) *outside* Anseo; the SDK posts that run to `POST /v1/ingest/run`, so it flows through the **same extraction → redaction → benchmark-contribution path** as a native `anseo prompt run`. Think OpenTelemetry-for-AI-search-visibility: a thin wrapper reads the provider's raw response, derives `provider`/`model`, and ships the run — without changing your inference logic, and **without ever interrupting your application**.

> This page documents the OSS-canonical instrumentation surface only: the three SDKs (Python/TypeScript/Go), the ingest wire shape, the consent + redaction boundary, and the canonical-suite hook. The language-agnostic contract lives in [`../sdk-spec.md`](../sdk-spec.md); per-package READMEs live under [`packages/`](https://github.com/leandropalazzo/anseo/tree/main/packages).

## When to use the SDK vs. native runs

| You run prompts… | Use |
|---|---|
| via Anseo (`anseo prompt run`, schedules, MCP `run_prompt`) | nothing — runs are already recorded and (if opted in) contributed |
| in your own application/pipeline, outside Anseo | the **instrumentation SDK** — ship each run to `/v1/ingest/run` |

The slug you record (`prompt_slug`) **must already be declared** in the project (`anseo prompt add --name <slug>`). The ingest API never auto-creates prompts — an undeclared slug returns `422 prompt_not_found`.

---

## Quickstart

All three SDKs talk to the same endpoint with the same wire shape (see [Wire shape](#wire-shape)). Pick your language.

### Python — `anseo-observe`

Zero runtime dependencies (standard-library `urllib` only). Source: [`packages/sdk-python/anseo_observe/`](https://github.com/leandropalazzo/anseo/tree/main/packages/sdk-python).

```python
import os
from anseo_observe import AnseoObserver, observe

obs = AnseoObserver(
    base_url="https://anseo.internal",
    api_key=os.environ["ANSEO_API_KEY"],
    project="Sunski",  # omit for single-project deployments
)

# Context-manager form — auto-detects provider + model from the response object.
with observe(obs, prompt_slug="best-polarized-sunglasses") as run:
    resp = openai_client.chat.completions.create(...)
    run.capture(resp)   # no monkeypatching — it only reads documented attributes
```

Decorator form (the wrapped function's return value is treated as the raw response):

```python
@observe(obs, prompt_slug="best-polarized-sunglasses")
def ask():
    return openai_client.chat.completions.create(...)
```

Strict / manual surface — use when you want to *read* each contribution status (e.g. a backfill):

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
```

Construction raises `AnseoConfigError` when `base_url`/`api_key` is missing; `observe_run` raises `AnseoApiError` (with `.status`, `.code`) on a non-2xx. Debug logging: `logging.getLogger("anseo").setLevel(logging.DEBUG)`.

### TypeScript — `@anseo/observe`

Zero runtime dependencies — uses the global `fetch` (Node ≥ 18, Deno, edge runtimes, browsers). Source: [`packages/sdk-typescript/`](https://github.com/leandropalazzo/anseo/tree/main/packages/sdk-typescript).

```ts
import { AnseoObserver } from "@anseo/observe";

const observer = new AnseoObserver({
  baseUrl: "https://anseo.internal",
  apiKey: process.env.ANSEO_API_KEY!,
  project: "Sunski", // omit for single-project deployments
});

const result = await observer.observeRun({
  promptSlug: "best-polarized-sunglasses",
  provider: "openai",
  model: "gpt-4o-2024-08-06",
  responseText: completion.choices[0].message.content ?? "",
  // optional: citationDomains: ["sunski.com"], observedRank: 1, observedAt: new Date(),
  // optional: contribute: true, // requires project KEK; consent controls sealing
});

// result fields are the snake_case wire shape (parity with the Python/Go SDKs)
console.log(result.run_id, result.contribution.status);
```

A one-liner `observeRun(config, run)` function is also exported for fire-and-forget use. Non-2xx responses reject with an `AnseoApiError` carrying `.status` and `.code`.

### Go

Standard library only. Source: [`packages/sdk-go/`](https://github.com/leandropalazzo/anseo/tree/main/packages/sdk-go).

```go
import observe "github.com/opengeo/opengeo/packages/sdk-go"

observer, _ := observe.New(observe.Config{
    BaseURL: "https://anseo.internal",
    APIKey:  os.Getenv("ANSEO_API_KEY"),
    Project: "Sunski", // omit for single-project deployments
})

result, err := observer.ObserveRun(ctx, observe.RunInput{
    PromptSlug:   "best-polarized-sunglasses",
    Provider:     "openai",
    Model:        "gpt-4o-2024-08-06",
    ResponseText: completionText,
    // optional: CitationDomains: []string{"sunski.com"}, ObservedRank: observe.Int(1), ObservedAt: time.Now(),
    // optional: Contribute: observe.Bool(true), // requires project KEK; consent controls sealing
})
if err != nil {
    // *observe.APIError carries .Status and .Code on non-2xx responses.
    log.Fatal(err)
}
fmt.Println(result.RunID, result.Contribution.Status)
```

> **Naming note.** The Go module is still published under the legacy `github.com/opengeo/opengeo/packages/sdk-go` import path (the import statement above reflects the real, current path); the TypeScript package already ships as `@anseo/observe`. Both SDKs send the canonical `X-Anseo-*` headers, and the server also accepts the legacy `X-OpenGEO-*` spellings for back-compat (see below). The Go import path migrates with the broader Anseo rebrand; this page uses canonical `anseo.ai` hostnames and `ANSEO_*` env-var conventions throughout.

---

## The `observe()` contract (Python)

`observe(observer, prompt_slug=..., [provider=], [model=], [observed_rank=], [citation_domains=], [contribute=])` works two ways from one object:

- **Context manager** — you run the LLM call, then `run.capture(resp)` auto-detects `provider`/`model` and extracts `response_text`. The run ships best-effort on a clean block exit. No `capture()` ⇒ nothing sent.
- **Decorator** — the wrapped function's return value is treated as the raw response and captured automatically.

Explicit `provider`/`model` always override auto-detection. If `model` cannot be determined and is not supplied, the run is **skipped** (logged at DEBUG) rather than sent with a bogus model.

### Auto-detect (read-only, no monkeypatching)

| Provider | Version floor | Detection | Text extraction |
|---|---|---|---|
| OpenAI | `openai>=1.0` | `response.object` starts with `chat.` / `== "response"` / `== "text_completion"`; or `response.model` starts with `gpt` | `response.output_text`, else `response.choices[0].message.content` |
| Anthropic | `anthropic>=0.21` | `response.type == "message"`, or `response.model` starts with `claude` | concatenated `.text` of each block in `response.content` |
| (string) | — | caller supplies `provider`/`model` | the string itself |

Anything else ⇒ `provider="unknown"` (the server validates) and the caller must supply `model`.

---

## Delivery semantics

Two surfaces over the same wire shape:

- **Best-effort** (`observe` decorator/context-manager; the Python `send`, TS one-liner) — **never raises into the host app**; returns the result on success or null/None on failure.
- **Strict** (`observe_run` / `observeRun` / `ObserveRun`) — returns the parsed result and **raises** on any non-2xx, for manual/synchronous control.

**At-most-once: the SDK never retries on ANY status, including 5xx.** A retry on 5xx would break at-most-once — the server may have processed the first request and timed out before responding, so a retry could double-record the run. On any error (network, timeout, decode, or server status) the best-effort surface logs and discards.

Logging (Python `anseo` logger): transport/timeout/decode errors and non-401 server statuses → **DEBUG**; `401 Unauthorized` (bad key) → **WARN** but still swallowed. If the wrapped call itself raises, nothing is sent and the original exception propagates unchanged.

---

## Wire shape

### Request — `POST /v1/ingest/run`

Headers:

| Header | Value |
|---|---|
| `content-type` | `application/json` |
| `X-Anseo-API-Key` | the operator's API key (canonical; server also accepts legacy `X-OpenGEO-API-Key`) |
| `X-Anseo-Project` | the project/brand name, **only when `project` is set** (canonical; legacy `X-OpenGEO-Project`) |

The API key is the **sole** auth — there is no separate SDK credential.

Body (snake_case; optional fields omitted when unset so server defaults apply):

```json
{
  "prompt_slug": "best-polarized-sunglasses",
  "provider": "openai",
  "model": "gpt-4o-2024-08-06",
  "response_text": "…",
  "citation_domains": ["sunski.com"],
  "observed_rank": 1,
  "observed_at": "2026-06-04T12:00:00+00:00",
  "contribute": true
}
```

`prompt_slug`, `provider`, and `model` are required; the rest are optional.
`contribute` defaults to `false` when omitted. `provider` validation is the
server's job — an unknown provider is sent through as-is (e.g. `"unknown"`),
not rejected client-side.

### Response — HTTP 202

```json
{
  "run_id": "01J…",
  "project_id": "01J…",
  "prompt_slug": "best-polarized-sunglasses",
  "provider": "openai",
  "observed_at": "2026-06-04T12:00:00Z",
  "contribution": { "status": "sealed" }
}
```

A run is **persisted (HTTP 202) when accepted**. The `contribution.status`
tells you what happened to the benchmark leg:

| `contribution.status` | Meaning |
|---|---|
| `sealed` | run recorded **and** the redacted benchmark payload was sealed for contribution |
| `skipped_not_opted_in` | run recorded; project has not opted into the public benchmark (or this run did not request contribution) |
| `redaction_rejected` (with `reason`) | run recorded; the redactor refused to seal (e.g. stale terms version, non-slug-safe slug) |

### Errors

Non-2xx bodies carry `{ "error": "<code>", "message": "<human text>" }`, e.g.
`400 validation_failed`, `401` (bad key), `403 kek_missing`, `422
prompt_not_found`, or `422 provider_not_supported`.

---

## Consent model — what gets contributed

Persisting a run and contributing it to the public benchmark are **two separate decisions**. An accepted ingested run is persisted; it is sealed for the public benchmark only when **all** of these hold:

1. **The project opted in.** Run `anseo benchmark optin` once per project (audited, durable). Check with `anseo benchmark status`; reverse with `anseo benchmark optout`. Without opt-in, the response reports `skipped_not_opted_in`.
2. **The project has a per-project encryption key (KEK).** A run that requests contribution without a KEK is rejected as `403 kek_missing` and is **not** recorded under a false promise of contribution.
3. **The current benchmark terms are accepted.** The redactor stamps each sealed payload with the consented `terms_version` and refuses to seal if the operator's accepted terms are stale (`redaction_rejected`).

Contribution is **off by default** and opt-in throughout. The SDK itself does not — and cannot — seal contributions; the consent/redaction gate is enforced **server-side** by the ingest path, identically to native runs.

---

## What's transmitted — the redaction boundary

When a run is sealed for the public benchmark, it is **redacted** from the full internal run down to a narrow public payload. This mirrors the redaction boundary applied to native runs — the SDK does not change it.

**Transmitted in a sealed benchmark payload:**

| Field | Notes |
|---|---|
| `prompt_slug` | must be slug-safe (lowercase ASCII + digits + hyphens) |
| `provider` | e.g. `openai`, `anthropic` |
| `model` | e.g. `gpt-4o-2024-08-06` |
| `observed_at_hour` | timestamp **rounded down to the hour** for k-anonymity (08:43 → 08:00) |
| `observed_rank` | optional |
| `citation_domains` | source domains observed in citations (deduplicated) |
| `project_hmac` | a keyed HMAC derived from the per-project KEK — a stable, **non-reversible** project identifier; **not** the brand name |
| `terms_version` | the benchmark terms the operator consented to (currently `v1-2026-05-28`) |

**Never transmitted (dropped at the boundary):**

- `brand_name` — the public payload carries no brand name; there is not even an accessor for it
- `raw_response_text` — the full model response text stays on your node
- `api_key_used`
- `ip_address`

The transmitted payload is intentionally constructable **only** by the server-side redactor — there is no client-side or back-door constructor, so a run cannot be contributed with un-redacted fields.

---

## Canonical-suite hook

The public benchmark is only useful as a dataset if contributors' runs are **apples-to-apples**. Two operators who each track "best vector database" only land in the same cohort if they use the **same `prompt_slug`**.

The mechanism is the slug itself:

- The `prompt_slug` you ingest is the join key. Runs sharing a slug (across operators) aggregate into the same benchmark cohort.
- The slug must be **declared in your project** before you can ingest against it (`anseo prompt add --name <slug>`), and it must be **slug-safe** (lowercase ASCII + digits + hyphens).
- To contribute comparable data, align your slugs to the **canonical GEO prompt suite** — a versioned list of standard prompt slugs (e.g. `geo-v1/best-vector-db`) shared by all contributors. Using a canonical slug is what puts your runs in the same cohort as everyone else's.

> **Current state.** The canonical GEO prompt suite is now published as a versioned artifact in [`./canonical-suite.md`](./canonical-suite.md), but the convenience surfaces from Story 40.5 are still pending: there is not yet a `GET /v1/suite/prompts` endpoint, `anseo suite list` / `anseo suite check`, or MCP suite-listing tool. You can already align your ingested runs to the published canonical slugs today; the dedicated API/CLI/MCP helpers will arrive in Epic 40.

Until then, the comparable-contribution recipe is:

1. `anseo prompt add --name <slug>` — declare the slug in your project.
2. `anseo benchmark optin` — opt the project into the public dataset (once).
3. Ensure the project has a KEK and current terms are accepted.
4. Instrument your code with the SDK, recording that exact `prompt_slug`.
5. Read `contribution.status` on the response to confirm runs are `sealed`.

---

## See also

- [`../sdk-spec.md`](../sdk-spec.md) — the language-agnostic SDK contract (the source of truth the three SDKs implement).
- [CLI manual](./cli.md) — `anseo prompt`, `anseo benchmark optin|optout|status`, API keys.
- [MCP manual](./mcp.md) — agent-facing tools over the same `/v1` API.
- [Deploy manual](./deploy.md) — standing up the node the SDK ships to.
