# Anseo

Anseo is the self-hostable observability stack for AI search visibility — track how your brand ranks in LLM responses, against your competitors, over time. Fully open source (MIT); runs entirely in your own deployment.

```bash
ogeo init                                  # scaffold opengeo.yaml
ogeo login openai                          # store a provider key (OS keychain / age file / env)
ogeo login anthropic
ogeo prompt run                            # run declared prompts × providers, extract + persist
ogeo report generate --format markdown     # summarize a recent window
ogeo check visibility --expect-rank-lte 3  # CI gate on ranking (FR-15)
ogeo dashboard open                        # open the local dashboard
```

The closed local loop:

```text
YAML config -> CLI prompt runs -> provider calls -> mention/citation extraction -> PostgreSQL persistence -> dashboard -> ogeo serve (or Docker Compose)
```

**v0.5.0** wrapped that loop in a full operability surface (the OSS GA): a programmable REST `/v1` API with auto-generated TypeScript + Python SDKs, scheduled runs with anomaly alerts and webhook/Slack/SMTP delivery, ClickHouse-backed analytics, seven provider adapters, a GitHub Action, and a redesigned dashboard.

**v0.6.0** closes Phase 3: an MCP server, a GEO recommendation engine, guided setup/deployment flows, plus a plugin SDK and a browser extension (both shipping as preview/substrate — see the caveats below). See [`CHANGELOG.md`](CHANGELOG.md).

## Deploying each tier

One multi-project core, three ways to run it — pick by how much you want running.

**Tier 0 — solo CLI.** Just `ogeo` against a Postgres you point it at; no long-running services.

```bash
export DATABASE_URL=postgres://opengeo:opengeo@localhost:5432/opengeo
ogeo init && ogeo login openai
ogeo prompt run
```

**Tier 1 — single binary (`ogeo serve`).** One process runs the REST API + the worker in-process. With no `DATABASE_URL` it provisions and supervises a **managed child Postgres** (nothing else to install); set `DATABASE_URL` to use your own. Binds `127.0.0.1:8080` by default.

```bash
ogeo serve                                         # managed child Postgres; API+worker on :8080
DATABASE_URL=postgres://… ogeo serve --port 8080   # …or bring your own Postgres
```

Bind to a non-loopback address (`--bind host:port`) only behind your own auth/network controls — a public bind with no API keys is refused. See [Production deployment](docs/production-deployment.md) for reverse-proxy/TLS guidance.

**Tier 2 — Docker Compose.** The full stack (API + worker + web + Postgres) from `infra/docker`.

```bash
cp infra/docker/.env.example infra/docker/.env     # defaults are fine for a local run
docker compose -f infra/docker/compose.yml up -d
docker compose -f infra/docker/compose.yml ps      # wait for healthy
```

Every published port binds to `127.0.0.1` by default; override with `OGEO_BIND_HOST` only behind your own network controls. ClickHouse analytics is connected separately via the dashboard's guided setup. See [Production deployment](docs/production-deployment.md) for Caddy/nginx reverse-proxy configs and the pre-launch checklist.

A single operator runs **multiple projects** (brands) per deployment; the CLI, web dashboard, and MCP server all thread the selected project through every call.

## Exposing Anseo safely (security baseline)

**Do not expose Anseo to a public network without a reverse proxy, TLS, and auth in front of it.** The OSS stack has no built-in authentication for the web dashboard or MCP surfaces; only the `/v1` REST API enforces per-project API keys. `ogeo serve` binds `127.0.0.1` by default; if you override that to a public interface it prints a non-blocking warning reminding you to put a proxy in front.

See **[docs/production-deployment.md](docs/production-deployment.md)** for:
- Copy-paste Caddy (automatic TLS) and nginx reverse-proxy configs with auth
- Docker Compose `OGEO_BIND_HOST` guidance
- The five-item pre-launch production checklist

## Repo model (inverted open-core, ADR-007)

This public `opengeo` repo is the **canonical OSS source of truth** — make OSS changes here. A private `opengeo-internal` repo overlays it via git submodule and adds commercial/Pro capabilities (premium hallucination/brand-accuracy verdicts, hosted-cloud infra) — **none of which appears in this repo**. See [`docs/open-core-boundary.md`](docs/open-core-boundary.md) for the full MIT-OSS vs private split.

## Layout

```text
apps/
  api/      Axum REST `/v1` API binary
  worker/   background worker (scheduled runs, alert/webhook delivery)
  cli/      `ogeo` CLI binary
  mcp/      MCP server binary
  web/      Next.js dashboard (the canonical Anseo dashboard)
crates/
  core/             shared core contracts (Secret, error/exit-code taxonomy, secret store)
  providers/        provider adapters (OpenAI, Anthropic, Gemini, Perplexity, Grok, Mistral, OpenRouter)
  extractors/       mention and citation extraction
  storage/          PostgreSQL persistence + migrations
  analytics/        analytics query layer (Postgres + ClickHouse)
  scheduler/        schedule evaluation
  benchmark/        public-benchmark consent + payload (client)
  recommendations/  GEO recommendation engine
  plugin-host/      plugin host + sandbox (ed25519 TOFU signing, capability catalog)
  plugin-manifest/  plugin manifest tooling
  wire-schema/      shared API/schema DTOs + OpenAPI generation
extension/          MV3 browser extension (preview)
packages/           generated TypeScript / Python / Go SDKs
infra/
  docker/           Docker Compose stack
  github-action/    `ogeo check` GitHub Action (bats + smoke tests)
```

## Toolchain

- Rust: `1.95.0`, edition 2021, pinned in `rust-toolchain.toml`
- Node: LTS; package manager `pnpm` (for `apps/web`)

## Verification

```bash
cargo build
cargo test

cd apps/web && pnpm install && pnpm build && pnpm lint
```

## Programmable surface (REST `/v1` + SDKs)

`apps/api` exposes a read + write REST API under `/v1`, authenticated with per-project API keys. OpenAPI is generated from `crates/wire-schema`, and the TypeScript + Python + Go SDKs in `packages/` are generated from that spec (a CI drift gate keeps them in sync).

```bash
ogeo api key create --name ci        # plaintext shown once
cargo run -p opengeo-api             # serve on :8080
```

## Scheduling, alerts & webhooks

YAML `v0.2` schedule definitions drive the background worker (at-most-once delivery), which surfaces visibility + citation anomalies and fans notifications out to webhooks (HMAC-signed, retry ladder, auto-disable), Slack, and SMTP.

## Analytics (ClickHouse)

A ClickHouse analytics backend with a Postgres↔ClickHouse parity test and live routes (`/v1/analytics/{citation-graph,heatmap,volatility}`). Migrate idempotently with `ogeo analytics migrate-to-clickhouse`.

## MCP server

`apps/mcp` is a Model Context Protocol server (stdio + HTTP/SSE) that lets an LLM client (Claude Desktop, Cursor, Zed) query and drive a project. Seven tools: `run_prompt`, `get_visibility`, `get_citations`, `list_trends`, `compare_brands`, `search_benchmarks`, `recommend`.

```bash
ogeo mcp serve
ogeo mcp install-config              # write a Claude Desktop / Cursor / Zed snippet
```

## GEO recommendations

The `recommendations` crate emits prioritized, reproducible recommendations (deterministic kinds plus LLM-assisted kinds behind a determinism allow-list + cost cap), each with a lifecycle (surfaced → acknowledged → acted/dismissed) and webhook events. Available via `ogeo recommend`, REST `/v1/recommendations`, and the MCP `recommend` tool.

## Preview surfaces

Two surfaces are in the tree but ship as **preview** — substrate and contracts are landed, but several behaviors are still mock-backed. Treat them as not-yet-production:

- **Plugin SDK** (`crates/plugin-host`, `crates/plugin-manifest`, `ogeo plugin`) — manifest validation, ed25519 TOFU signing + revocation, namespace claims, and the capability catalog are real; sandbox execution, the registry, and marketplace surfaces are mock-backed.
- **Browser extension** (`extension/`) — the MV3 manifest, service worker, paste-token bind, and the five-layer privacy invariant are real; per-site adapters, Shadow-DOM overlays, citation chips, popup, and the prompt-analysis classifier are mock-backed.

## CLI error mapping

CLI commands return structured exit codes (stable within a major version): `0` success · `1` visibility check failed · `2` provider error · `64` config error · `65` data error · `66` auth/permission · `70` internal. On `2`, the `error_kind` field carries `provider_unauthorized` / `provider_rate_limited` / `provider_timeout` / `provider_5xx` / `provider_invalid_response` / `network_error`. Defined in `crates/core/src/error.rs`.

## Privacy posture

Localhost-first. Provider keys stay local, raw responses stay in your deployment, and Anseo sends no telemetry to Anseo-controlled services except via explicit opt-in (e.g. the public benchmark).

## License

MIT — see [LICENSE](./LICENSE).
