# Anseo Manuals — Workspace-Full (Operator Edition)

> **This is the workspace-full variant** — the complete operator manual including Pro/overlay capabilities delivered via the `anseo-internal` overlay. It is staged here for relocation to `anseo-internal/docs-internal/` by a maintainer. It must **never** be published to the public OSS documentation site.

Anseo is open-source AI-search-visibility **observability infrastructure** — "Prometheus + Grafana for AI search." It ships **three coordinated surfaces over one engine**, and a core promise: **CLI ⇄ Web ⇄ MCP parity** — the same data and operations, reachable however you work.

| Manual | Surface | For | Read |
|---|---|---|---|
| [CLI](./cli.md) | `anseo` binary | operators, CI/CD, scripts | every command + use case |
| [MCP](./mcp.md) | `anseo-mcp` server | AI agents / assistants | every tool + how to connect |
| [Web](./web.md) | Next.js dashboard | humans (read + control) | every route + use case |
| [Instrumentation](./instrumentation.md) | `anseo-observe` SDKs + `/v1/ingest/run` | developers shipping external LLM runs | quickstarts, consent, canonical-suite hook |
| [Deploy](./deploy.md) | tiers 0–2 (+ Pro Tier 3) | operators standing up a node | how to run each tier |

> The narrative onboarding manual (concepts, install, the Phase-1 closed loop) lives at [`../../release-manual.html`](../../release-manual.html). These three docs are the **surface references** — exhaustive, use-case-indexed.

## The architecture in one picture

```
                    anseo.yaml  (canonical declaration — "what should happen")
                          │
        ┌─────────────────┼──────────────────┐
        │                 │                  │
   anseo (CLI)         anseo-mcp         apps/web (dashboard)
        │            (AI agents)              │
        └───────────────► /v1 REST API ◄──────┘
                          │
                 Postgres + ClickHouse  ("what was observed")
                          │
              [Pro overlay: anseo-internal crates]
              orgs / RLS / RBAC / SSO / billing / cloud
```

- **`anseo.yaml` is the source of truth.** It declares Prompts, Providers, Brand, Competitors, Schedules, Webhooks. The database stores *observations*; the YAML stores *intent*. Removing a prompt never deletes its history.
- **Everything funnels through the `/v1` REST API.** The MCP server proxies to it; the web app reads from it; the CLI uses the same engine. This is why parity holds.
- **Reproducibility is foundational (NFR-1):** every Prompt Run records provider, model, prompt text, request params, and the full raw response. Given the same YAML + stored data, results are stable.

## Choose your surface by use case

| I want to… | Use |
|---|---|
| Bootstrap a project, run prompts, gate CI | **CLI** (`anseo init`, `anseo prompt run`, `anseo check visibility`, `anseo audit --fail-on`) |
| Let an AI assistant query my visibility / run audits | **MCP** (`run_prompt`, `get_visibility`, `compare_brands`, `audit`, …) |
| Explore trends, triage anomalies, review evidence | **Web** (`/visibility`, `/alerts`, `/runs/[id]`, `/recommendations`) |
| Ship LLM runs you executed outside Anseo into the same pipeline | **Instrumentation** (`anseo-observe` SDKs → `/v1/ingest/run`) |
| Automate recurring monitoring | **CLI/Web** schedules + **webhooks** |
| Pipe data into BI / other tools | **CLI** `--format json` / **REST API** (see CLI manual → API keys) |
| Manage an org with multiple teams / RLS / RBAC | **Pro overlay** (orgs, roles, row-level security) |
| Enforce SSO for your org | **Pro overlay** (SSO/SAML/OIDC, `anseo-internal`) |
| Manage subscriptions, seat billing, invoices | **Pro overlay** (billing module, `anseo-internal`) |
| Get hallucination / brand-accuracy verdicts on AI claims | **Pro overlay** (premium verdicts, `anseo-internal`) |

## Open-core boundary

The **MIT OSS core** covers: data collection, extraction, analytics, crawler ingestion, audit heuristics, recommendations, multi-project support, `anseo serve`, the plugin host and marketplace OSS parts, and all SDKs.

### Premium / Pro overlay (anseo-internal)

Capabilities delivered via the `anseo-internal` private overlay crate set. Requires `entitlement = premium_enabled` or a valid Pro license key.

| Capability | Overlay crate | Notes |
|---|---|---|
| Orgs / multi-team | `og-orgs` | Org-scoped projects, member management |
| Row-level security (RLS) | `og-rls` | Postgres RLS policies per org/team |
| RBAC | `og-rbac` | Role definitions, permission grants |
| SSO / SAML / OIDC | `og-sso` | IdP integration, JIT provisioning |
| Billing | `og-billing` | Stripe integration, seat mgmt, invoices |
| Hallucination / brand-accuracy verdicts | `og-verdicts` | Premium AI claim verification; requires `entitlement = premium_enabled` |
| Cloud infrastructure | `og-cloud` | Hosted-cloud deployment, managed infra |
| Audit compliance logs | `og-audit-log` | Immutable audit trail for compliance |
