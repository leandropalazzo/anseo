# @anseo/sdk

TypeScript client for the Anseo REST API. Auto-generated from
`crates/wire-schema/openapi.json` by orval; do not edit
`src/client.ts` or anything under `src/schemas/` by hand.

## Install

```bash
npm install @anseo/sdk
```

(Or `pnpm add @anseo/sdk` / `yarn add @anseo/sdk`.)

## Usage

```ts
import { configure } from "@anseo/sdk/runtime";
import { listRuns, createPromptRun } from "@anseo/sdk";

configure({
  baseUrl: "http://127.0.0.1:8080",
  apiKey: process.env.ANSEO_API_KEY,
});

const runs = await listRuns({ limit: 25 });

const created = await createPromptRun({
  promptName: "vector-db",
  provider: "mock",
});
```

The runtime mutator (`@anseo/sdk/runtime`) wires:

- **Base URL** — set once via `configure({ baseUrl })`.
- **Auth** — the configured `apiKey` is sent in the `X-Anseo-API-Key`
  header on every request (architecture §5.1). The Anseo API does
  not accept `Authorization: Bearer`.
- **JSON content negotiation** — sets `Accept` and `Content-Type`
  defaults; skips the `Content-Type` default when sending FormData,
  Blob, URLSearchParams, ArrayBuffer, or a ReadableStream.
- **Error translation** — non-2xx responses throw
  `AnseoApiError(message, status, body)`; network errors throw the
  same type with `status=0`.

## Regenerating

After any change to `crates/wire-schema/openapi.json`:

```bash
pnpm regenerate    # delegates to: make -C ../../infra/codegen ts
```

The CI drift gate at `infra/codegen/tests/drift.sh` blocks merges if
this package is out of sync with the canonical spec.
