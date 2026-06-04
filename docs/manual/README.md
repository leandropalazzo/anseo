# OpenGEO Manuals

OpenGEO is open-source AI-search-visibility **observability infrastructure** — "Prometheus + Grafana for AI search." It ships **three coordinated surfaces over one engine**, and a core promise: **CLI ⇄ Web ⇄ MCP parity** — the same data and operations, reachable however you work.

| Manual | Surface | For | Read |
|---|---|---|---|
| [CLI](./cli.md) | `ogeo` binary | operators, CI/CD, scripts | every command + use case |
| [MCP](./mcp.md) | `opengeo-mcp` server | AI agents / assistants | every tool + how to connect |
| [Web](./web.md) | Next.js dashboard | humans (read + control) | every route + use case |
| [Deploy](./deploy.md) | tiers 0–2 | operators standing up a node | how to run each tier |

> The narrative onboarding manual (concepts, install, the Phase-1 closed loop) lives at [`../release-manual.html`](../release-manual.html). These three docs are the **surface references** — exhaustive, use-case-indexed.

## The architecture in one picture

```
                    opengeo.yaml  (canonical declaration — "what should happen")
                          │
        ┌─────────────────┼──────────────────┐
        │                 │                  │
   ogeo (CLI)        opengeo-mcp         apps/web (dashboard)
        │            (AI agents)              │
        └───────────────► /v1 REST API ◄──────┘
                          │
                 Postgres + ClickHouse  ("what was observed")
```

- **`opengeo.yaml` is the source of truth.** It declares Prompts, Providers, Brand, Competitors, Schedules, Webhooks. The database stores *observations*; the YAML stores *intent*. Removing a prompt never deletes its history.
- **Everything funnels through the `/v1` REST API.** The MCP server proxies to it; the web app reads from it; the CLI uses the same engine. This is why parity holds.
- **Reproducibility is foundational (NFR-1):** every Prompt Run records provider, model, prompt text, request params, and the full raw response. Given the same YAML + stored data, results are stable.

## Choose your surface by use case

| I want to… | Use |
|---|---|
| Bootstrap a project, run prompts, gate CI | **CLI** (`ogeo init`, `ogeo prompt run`, `ogeo check visibility`, `ogeo audit --fail-on`) |
| Let an AI assistant query my visibility / run audits | **MCP** (`run_prompt`, `get_visibility`, `compare_brands`, `audit`, …) |
| Explore trends, triage anomalies, review evidence | **Web** (`/visibility`, `/alerts`, `/runs/[id]`, `/recommendations`) |
| Stand up a node (solo CLI / single binary / Compose) | **[Deploy](./deploy.md)** (the three tiers) |
| Automate recurring monitoring | **CLI/Web** schedules + **webhooks** |
| Pipe data into BI / other tools | **CLI** `--format json` / **REST API** (see CLI manual → API keys) |

## Open-core boundary

The **MIT OSS core** covers: data collection, extraction, analytics, crawler ingestion, audit heuristics, recommendations, multi-project support, `ogeo serve`, the plugin host and marketplace OSS parts, and all SDKs. This manual set documents only OSS-canonical surfaces.
