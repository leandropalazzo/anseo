# @opengeo/sdk

TypeScript client for the OpenGEO REST API. Auto-generated from
`crates/wire-schema/openapi.json` by orval; do not edit
`src/client.ts` or anything under `src/schemas/` by hand.

## Install

```bash
npm install @opengeo/sdk
```

(Or `pnpm add @opengeo/sdk` / `yarn add @opengeo/sdk`.)

## Usage

```ts
import { configure } from "@opengeo/sdk/runtime";
import { listRuns, createPromptRun } from "@opengeo/sdk";

configure({
  baseUrl: "http://127.0.0.1:8080",
  apiKey: process.env.OPENGEO_API_KEY,
});

const runs = await listRuns({ limit: 25 });

const created = await createPromptRun({
  promptName: "vector-db",
  provider: "mock",
});
```

The runtime mutator (`@opengeo/sdk/runtime`) wires:

- **Base URL** — set once via `configure({ baseUrl })`.
- **Auth** — the configured `apiKey` is sent in the `X-OpenGEO-API-Key`
  header on every request (architecture §5.1). The OpenGEO API does
  not accept `Authorization: Bearer`.
- **JSON content negotiation** — sets `Accept` and `Content-Type`
  defaults; skips the `Content-Type` default when sending FormData,
  Blob, URLSearchParams, ArrayBuffer, or a ReadableStream.
- **Error translation** — non-2xx responses throw
  `OpenGeoApiError(message, status, body)`; network errors throw the
  same type with `status=0`.

## Regenerating

After any change to `crates/wire-schema/openapi.json`:

```bash
pnpm regenerate    # delegates to: make -C ../../infra/codegen ts
```

The CI drift gate at `infra/codegen/tests/drift.sh` blocks merges if
this package is out of sync with the canonical spec.
