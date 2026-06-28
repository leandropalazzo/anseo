# Anseo Instrumentation SDK — Core Spec

**Status:** locked (Story 40.2). Language-agnostic contract implemented by the
Python SDK (40.2) and the TypeScript/Go SDKs (40.3). The reference
implementation is `packages/sdk-python/anseo_observe/`.

The SDK is the **client** side of the Run-Ingestion API (Story 40.1,
`apps/api/src/routes/ingest.rs`). It instruments an application's
externally-executed LLM calls and ships each run, best-effort, to
`POST /v1/ingest/run`, so the run flows through the same
extraction → redaction → benchmark-contribution path as a native run.

The design goal is OpenTelemetry-for-LLM-visibility: a thin wrapper reads the
provider's raw response, derives `provider`/`model`, and POSTs the run — without
the host application changing its inference logic, and without the SDK ever
interrupting that application.

---

## 1. Design invariants

1. **Minimal dependencies.** The SDK MUST NOT pull in heavy ML/HTTP frameworks.
   The Python reference uses the standard-library HTTP client only (zero runtime
   deps); other languages use their stdlib/built-in client.
2. **No monkeypatching.** The SDK MUST NOT patch, wrap, or import the OpenAI /
   Anthropic SDKs. Auto-detection reads documented attributes off the response
   object the caller hands it. If auto-detect fails, the caller supplies
   `provider`/`model` explicitly.
3. **Best-effort, at-most-once delivery.** Observability MUST NOT crash or
   block the host app, and MUST NOT double-record a run. See §4.
4. **No raw payload leaves the client beyond what 40.1 already accepts.** The
   SDK sends only the fields in §3. The redaction/consent gate is enforced
   server-side by 40.1 (the redactor drops `brand_name`, `api_key_used`,
   `ip_address`, etc.); the SDK does not — and cannot — seal contributions
   itself. The consent flag is layered on in Story 40.4.

---

## 2. Configuration

| Field      | Required | Notes |
|------------|----------|-------|
| `base_url` | yes      | API origin, e.g. `https://anseo.internal`. Trailing slash normalized. |
| `api_key`  | yes      | The operator's API key. Sole auth — there is no separate SDK credential. |
| `project`  | no       | Brand/project name for the project-scope header. Omit for single-project deployments (server falls back to the sole active project). |
| `timeout`  | no       | Per-request timeout (default 30s). |

A missing `base_url` or `api_key` is a **configuration error raised eagerly at
construction**, never deferred to a call (Python: `AnseoConfigError`).

`endpoint override`: pointing `base_url` at a mock or staging origin is the
supported override mechanism; there is no separate endpoint config.

---

## 3. Wire shape

### Request — `POST /v1/ingest/run`

Headers:

| Header             | Value |
|--------------------|-------|
| `content-type`     | `application/json` |
| `x-anseo-api-key`  | the operator's API key (canonical; server also accepts legacy `x-opengeo-api-key`) |
| `x-anseo-project`  | the project name, **only when `project` is set** (canonical; legacy `x-opengeo-project`) |

Body (snake_case; optional fields omitted when unset so server defaults apply):

```json
{
  "prompt_slug": "best-polarized-sunglasses",   // required, slug-safe
  "provider": "openai",                          // required; "unknown" if undetectable
  "model": "gpt-4o-2024-08-06",                  // required
  "raw_response": { "text": "…" },               // canonical provider-native JSON
  "metadata": { "trace_id": "abc-123" },         // optional caller metadata
  "response_text": "…",                          // optional compatibility field for early clients
  "citation_domains": ["sunski.com"],            // optional
  "observed_rank": 1,                            // optional
  "observed_at": "2026-06-04T12:00:00+00:00",    // optional ISO-8601; defaults to server now
  "contribute": true                             // optional; defaults false
}
```

The request MUST include at least one payload surface: canonical
`raw_response`, compatibility `response_text`, or both. New SDKs should
populate `raw_response` and treat `response_text` as a compatibility shim.

`contribute` is a per-run opt-in to the benchmark contribution path. Omit it to
preserve the safe default (`false`). A `true` value requires a per-project KEK at
request time; the durable project opt-in then controls whether the accepted run
seals as `sealed` or reports `skipped_not_opted_in`.


`prompt_slug` MUST already be declared in the project — 40.1 returns `422
prompt_not_found` for an undeclared slug (no auto-create). `provider` validation
is server-authoritative: unsupported providers return `422 provider_not_supported`.

### Response — `IngestRunResponse` (HTTP 202)

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

`contribution.status` is the internally-tagged `ContributionStatus` from 40.1:
`sealed` · `skipped_not_opted_in` · `kek_missing` ·
`{ "status": "redaction_rejected", "reason": "…" }`. A run is **persisted**
(HTTP 202) when accepted; if `contribute: true` lacks a project KEK, the request
is rejected as `403 kek_missing` before persistence.

### Error responses

Non-2xx bodies carry `{ "error": "<code>", "message": "<human text>" }`, e.g.
`400 validation_failed`, `401` (bad key), `422 prompt_not_found`, `422
provider_not_supported`, `403 kek_missing` when `contribute: true` has no
project KEK, or `429 rate_limited`. The `429` guard is currently an in-process
per-project limiter on the API node, not a distributed cross-node quota.

---

## 4. Delivery semantics

Two surfaces over the same wire shape:

- **Strict** (`observe_run`): returns the parsed result; **raises** on any
  non-2xx. For manual / synchronous control (e.g. a backfill that wants to read
  each `contribution.status`).
- **Best-effort** (`send` and the `observe` decorator/context-manager): never
  raises into the host app, returns the result on success or a null/None on
  failure.

**At-most-once: never retry on ANY status, including 5xx.** A retry on 5xx
breaks at-most-once — the server may have processed the first request and timed
out before responding, so a retry could double-record the run. On any error
(network, timeout, decode, or server status) the best-effort surface logs and
discards.

Logging:

- transport / timeout / decode errors and non-401 server statuses → **DEBUG**.
- `401 Unauthorized` → **WARN** (the operator's key is wrong; surface it loudly)
  but still swallowed — the application is never interrupted.
- Diagnostics are emitted on the `anseo` logger (Python honours `DEBUG=anseo`
  via a logging handler on that logger).

If the **wrapped call itself raises**, nothing is sent (there is no run to
record) and the original exception propagates unchanged.

---

## 5. The `observe` contract

`observe(observer, prompt_slug=..., [provider=], [model=], [observed_rank=],
[citation_domains=], [contribute=])` works two ways from one object:

- **Context manager** — the caller runs the LLM call, then `run.capture(resp)`
  auto-detects `provider`/`model` and extracts `response_text`. The run ships
  best-effort on a clean block exit. No `capture()` ⇒ nothing sent.
- **Decorator** — the wrapped function's return value is treated as the raw
  response and captured automatically.

Explicit `provider`/`model` always override auto-detection. If `model` cannot be
determined and is not supplied, the run is skipped (logged at DEBUG) rather than
sent with a bogus model.

### Auto-detect attribute paths (read-only)

| Provider  | Version floor    | provider/model detection | text extraction |
|-----------|------------------|--------------------------|-----------------|
| OpenAI    | `openai>=1.0`    | `response.object` starts with `chat.` / `== "response"` / `== "text_completion"`; `response.model` | `response.output_text`, else `response.choices[0].message.content` |
| Anthropic | `anthropic>=0.21`| `response.type == "message"` or `response.model` starts with `claude`; `response.model` | concatenated `.text` of each block in `response.content` |
| (string)  | —                | caller supplies provider/model | the string itself |

Anything else ⇒ `provider="unknown"` (server validates) and the caller must
supply `model`.

---

## 6. Packaging

- Python: `packages/sdk-python/`, importable as `anseo_observe`, distributed as
  `anseo-observe` (placeholder CI; not a live PyPI publish in 40.2).
- Quality gates: `ruff check` + `mypy --strict` clean on the package source;
  unit tests run against a mocked transport (no real network). CI job:
  `sdk-python-lint-test`.
