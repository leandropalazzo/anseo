# @opengeo/observe

Thin instrumentation SDK to send **externally-executed** LLM runs to OpenGEO's
Run-Ingestion API (`POST /v1/ingest/run`). The OpenTelemetry pattern, minus the
ceremony: you already ran a prompt against a provider outside OpenGEO — this
posts that run so it flows through the same extraction → redaction →
benchmark-contribution path as a native run.

- **Zero runtime dependencies** — uses the global `fetch` (Node ≥ 18, Deno,
  edge runtimes, browsers).
- Sends the API key as `X-OpenGEO-API-Key` and scopes the run with
  `X-OpenGEO-Project` (brand name).

## Install

```bash
npm install @opengeo/observe
```

## One-liner integration

```ts
import { observeRun } from "@opengeo/observe";

await observeRun(
  { baseUrl: "https://opengeo.internal", apiKey: process.env.OPENGEO_API_KEY!, project: "Sunski" },
  { promptSlug: "best-polarized-sunglasses", provider: "openai", model: "gpt-4o-2024-08-06", responseText: completionText },
);
```

## Reusable client

```ts
import { OpenGeoObserver } from "@opengeo/observe";

const observer = new OpenGeoObserver({
  baseUrl: "https://opengeo.internal",
  apiKey: process.env.OPENGEO_API_KEY!,
  project: "Sunski", // omit for single-project deployments
});

const result = await observer.observeRun({
  promptSlug: "best-polarized-sunglasses",
  provider: "openai",
  model: "gpt-4o-2024-08-06",
  responseText: completion.choices[0].message.content ?? "",
  // optional:
  // citationDomains: ["sunski.com"],
  // observedRank: 1,
  // observedAt: new Date(),
});

// result.contribution tells you whether benchmark data was sealed.
// { status: "sealed" } | { status: "skipped_not_opted_in" }
// | { status: "kek_missing" } | { status: "redaction_rejected", reason }
console.log(result.runId, result.contribution.status);
```

Non-2xx responses reject with an `OpenGeoApiError` carrying `.status` and `.code`.

## Develop

```bash
npm install
npm run typecheck
npm run lint
npm test
```
