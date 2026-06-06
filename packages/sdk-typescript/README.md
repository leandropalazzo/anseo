# @anseo/observe

Thin instrumentation SDK to send **externally-executed** LLM runs to Anseo's
Run-Ingestion API (`POST /v1/ingest/run`). The OpenTelemetry pattern, minus the
ceremony: you already ran a prompt against a provider outside Anseo — this posts
that run so it flows through the same extraction → redaction →
benchmark-contribution path as a native run.

TypeScript port of the Python reference (`packages/sdk-python/anseo_observe`),
implementing the same language-agnostic spec in `docs/sdk-spec.md`.

- **Zero runtime dependencies** — uses the global `fetch` (Node ≥ 18).
- Sends the API key as `x-anseo-api-key` and scopes the run with
  `x-anseo-project` (brand name).
- **Best-effort, at-most-once**: `send`/`observe` never throw into your app and
  never retry.

## Install

```bash
npm install @anseo/observe
```

## Wrap a call (auto-detect, best-effort)

`observe` runs your call, auto-detects provider/model + extracts text from the
response, and ships the run best-effort. It returns your value unchanged; if the
call throws, nothing is sent.

```ts
import { AnseoObserver, observe } from "@anseo/observe";

const observer = new AnseoObserver({
  baseUrl: "https://anseo.internal",
  apiKey: process.env.ANSEO_API_KEY!,
  project: "Sunski", // omit for single-project deployments
});

const resp = await observe(
  observer,
  { promptSlug: "best-polarized-sunglasses" },
  () => client.chat.completions.create({ /* ... */ }),
);
```

For manual control, use `startRun` then `capture` + `ship`.

## Strict client (read each contribution status)

```ts
const result = await observer.observeRun({
  promptSlug: "best-polarized-sunglasses",
  provider: "openai",
  model: "gpt-4o-2024-08-06",
  responseText: resp.choices[0].message.content ?? "",
  // optional: citationDomains, observedRank, observedAt
});

// { status: "sealed" } | { status: "skipped_not_opted_in" }
// | { status: "kek_missing" } | { status: "redaction_rejected", reason }
console.log(result.runId, result.contribution.status);
```

`observeRun` rejects with an `AnseoApiError` (carrying `.status` and `.code`) on
a non-2xx; a missing `baseUrl`/`apiKey` throws `AnseoConfigError` at
construction. Enable diagnostics with `DEBUG=anseo`.

## Develop

```bash
npm install
npm run typecheck
npm run lint
npm test
```
