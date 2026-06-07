# Anseo Dashboard (`apps/web`)

The canonical Anseo web dashboard — a Next.js (App Router) + React + Tailwind app
that renders the analytics, reproducibility, and control-plane surfaces for a
project. It is **server-rendered**: every page fetches the Anseo `/v1` REST API on
each request (`cache: no-store`), so it needs a reachable API to show real data.

This is distinct from the public marketing/benchmark site, which lives in the
separate [`anseo-web`](https://github.com/leandropalazzo/anseo-web) repo.

## Toolchain

- Node: LTS pinned in `.nvmrc`
- Package manager: `pnpm`

## Configuration

The dashboard reads two environment variables at runtime:

| Variable               | Default                  | Purpose                                              |
|------------------------|--------------------------|-----------------------------------------------------|
| `ANSEO_API_BASE_URL`   | `http://localhost:8080`  | Base URL of the Anseo `/v1` REST API.               |
| `ANSEO_API_KEY`        | _(none)_                 | Per-project API key sent as `X-Anseo-API-Key` on SSR fetches. |

## Run it against a backend

Pick one backend, then start the dev server.

**A. Against `anseo serve` (Tier 1, simplest).** In one terminal run the API
(`anseo serve` brings up `/v1` on `127.0.0.1:8080` with a managed Postgres); then:

```bash
pnpm install
ANSEO_API_BASE_URL=http://127.0.0.1:8080 \
ANSEO_API_KEY=<your-key> \
pnpm dev          # http://localhost:3000
```

Mint a key with `anseo api key create --name dashboard` (printed once).

**B. Against the Docker stack (Tier 2).** The full stack already runs this
dashboard as the `web` service on `127.0.0.1:5173` — see
[`infra/docker/`](../../infra/docker/). Use that when you want the whole stack.

**C. Against the mock API (no backend, for UI work).** The Playwright harness
boots a mock API; you can reproduce it by hand:

```bash
node tests/e2e/mock-api-server.mjs          # serves 127.0.0.1:8787
# in another terminal:
ANSEO_API_BASE_URL=http://127.0.0.1:8787 ANSEO_API_KEY=e2e-test-key pnpm dev
```

## Verification

```bash
pnpm build
pnpm lint
pnpm test          # vitest (unit/component)
pnpm test:e2e      # Playwright (boots the mock API + dev server itself)
```

## Notes

- YAML (`anseo.yaml`) is the source of truth for project configuration; the
  dashboard reads it through the API.
- The dashboard has no built-in auth — never expose it on a public interface
  without a reverse proxy + auth in front. See the root
  [`docs/production-deployment.md`](../../docs/production-deployment.md).
