# Anseo Web Manual (Dashboard)

The dashboard (Next.js 16 App Router, `apps/web/app`) is the human surface — **read** analytics + **control** operations + **trust/evidence** views. It mirrors the CLI/MCP over the same `/v1` API.

**Cross-cutting behaviors**
- **Demo-data contract:** live data renders if present; if empty and `OGEO_DEMO=1`, mock data renders behind a visible **demo badge**; otherwise an **empty state** that usually points at the `anseo …` command to produce the data.
- **CLI parity:** empty states and "copy as command" affordances surface the equivalent `anseo` command throughout. Some actions are intentionally **CLI-only** (e.g. plugin install, bulk recommendation actions) and shown as disabled affordances, not hidden.
- **Graceful degradation:** a failed fetch yields an empty state, not a 500.

Legend: **R** read · **C** control · **S** setup/infra.

---

## Analytics — read surfaces

### `/` — Overview (R)
Morning-glance home. KPI hero (brand avg rank, mentions, success rate, run counts, provider count), anomaly timeline, sparkline stat tiles (rank, success rate, runs·7d, avg latency), and roll-up cards that link out: visibility-by-provider → `/visibility`, recent runs → `/runs`, top citations → `/citations`, competitor share-of-voice → `/competitors`, top recommendations → `/recommendations`, summary-by-tag → `/prompts`.
- **Use case:** daily health check + jump-off to every detail surface.

### `/visibility` — Visibility (R)
Tabs: **By prompt** (prompt picker, 1/7/30-day window, per-provider trend chart) and **Overall** (brand-wide matrix + 30-day trend).
- **Use case:** track how the brand ranks/appears per tracked prompt and overall. URL params `?prompt=` / `?days=`.

### `/competitors` — Competitors (R)
Share-of-voice chart (7d), head-to-head (two competitors), movers, top-4 competitor tiles, "where competitors win" win/loss table. Sends primary + top-5 competitors to `/v1/comparisons` (capped at 6 brands).
- **Use case:** competitive positioning per provider.

### `/citations` — Citations (R)
Tabs: **table** (domains + frequency + trend sparkline + citation score), **graph** (provider→domain network), **domains** (by source type).
- **Use case:** see which domains AI engines cite when answering your prompts.

### `/sentiment` — Sentiment (R)
7/30/90-day window; per-entity cards with avg score /100 and positive/neutral/negative share bars.
- **Use case:** monitor mention tone per entity. Empty state cites `anseo report --since 30d`.

### `/crawlers` — Crawlers (R + connect-source control)
7/30/90-day window; verified AI-crawler activity table (hits/verified/errors), top crawled paths, crawl-to-refer ratio table (handles `crawls_only` state), and a `ConnectSource` ingest panel.
- **Use case:** see which AI crawlers hit your site and whether crawls convert to referrals. Empty state cites `anseo crawlers --ratio`.

---

## Reproducibility / evidence surfaces

### `/runs` — Prompt Runs (R)
Newest-first table; tabs (all / failed / anomalies), provider filter chips, **CSV download**.
- **Use case:** browse/filter run history, export, drill into a run.

### `/runs/[id]` — Run Detail (R · evidence)
Header (run id, timestamp, prompt/provider/model, status) + actions **Copy id**, **CLI**, **Re-run**. Tabs: **Response** (response + diff + mentions), **Mentions** matrix, **Citations**, **Raw** payload, **Provenance** (step trace).
- **Use case:** the core reproducibility/evidence record — exactly what a provider returned, what was extracted, and the provenance chain.

---

## Control plane — operate

### `/prompts` — Prompts (C)
Prompt list + editor (name/text/tags; Save/Delete/Discard), an **AI generator** (pick a configured provider → suggest drafts → review → add), and a "copy as command" emitting `anseo run --prompt "…"`.
- **Use case:** CRUD prompts; generate prompt ideas grounded in brand/competitors. Renaming warns it re-derives identity (allowed only before first run).

### `/schedules` — Schedules (C)
Create a prompt × provider matrix on a cadence (form fans out across configured providers); run-now.
- **Use case:** automate recurring runs.

### `/alerts` — Alerts (C/R)
Tabs: **Inbox** (7-day anomalies) and **Rules** (alert rules).
- **Use case:** triage anomalies; manage alert rules.

### `/audit` — Site Audit (C/R)
`AuditRunner` (target defaults to brand `site_url`) + "past audits" history (when/target/pages/score-/100). Heuristics are **open, in-tree**.
- **Use case:** run an on-demand citation-readiness audit; review past scores. (Same engine as `anseo audit` and the MCP `audit` tool.)

### `/recommendations` — Recommendations (C/R)
`GenerateButton`, an "adoption by kind" panel (acted vs dismissed, % acted), and recommendation cards (priority, **NDP marker**, summary, kind/state/confidence) with inline Mark done / Dismiss.
- **Use case:** review & act on GEO recommendations; see which kinds actually move visibility. Empty state cites `anseo recommend generate`. Bulk actions are **CLI-only** (shown disabled).

### `/recommendations/[id]` — Recommendation Detail (C/R · evidence)
Header with priority, NDP marker, state, **reproducibility class**, engine version; an **NDP disclaimer** for non-deterministic recs ("directional, not guaranteed"); **Evidence & traceability** chips linking to source runs + citations (an empty traceability block is a render error); **lifecycle actions** (mark acted with evidence, etc.).
- **Use case:** understand the evidence behind a recommendation and record decisions.

---

## Integration / trust

### `/mcp` — MCP Server (R)
Searchable tool catalog ("the same surfaces as the Web UI, served by the MCP server") with example JSON calls + a Claude Desktop config snippet (`anseo mcp serve`), plus a recent-calls activity log.
- **Use case:** discover MCP tools and wire up a client. See the [MCP manual](./mcp.md).

### `/mcp/[tool]` — MCP Tool Detail (R)
Per-tool stats: total calls, error rate, p50/p95 latency, invocation history.
- **Use case:** monitor a single MCP tool's reliability.

### `/marketplace` + `/marketplace/[slug]` — Plugins (R + install control)
Plugin grid; detail page with capability block + install (installed/update pills or install sheet) and CLI fallback `anseo plugin install <slug>@<version>`.
- **Use case:** discover & install plugins (install runs via the CLI).

---

## Setup / infra

### `/settings` — Settings (C)
Sections: **Providers & keys**, **Brand & competitors**, **Privacy posture**, **Deployment**, **Extractors**, plus **Re-run onboarding**.
- **Use case:** central configuration hub.

### `/setup` — Deployment Setup (S/R)
Infra status cards: Postgres, Worker, ClickHouse (+docker), ETL progress (state/batches/heartbeat), API keys, webhook target.
- **Use case:** verify local/deployed infrastructure is healthy.

### `/setup/clickhouse/connect` — Connect Remote ClickHouse (S/C)
`ConnectForm` (provider preset + connection details; probes before saving).
- **Use case:** point Anseo at managed ClickHouse when local Docker isn't available.

---

## Dev / onboarding

### `/onboarding` — Initialize Anseo (C, first-run)
Five-step wizard mirroring `anseo init`: Initialize project → Connect providers → Configure brand → First prompt run → Schedule & alerts. Gated by an onboarded flag (bounces to `/` if already onboarded).
- **Use case:** guided first-run setup.

### `/dev` — Plugin Dev (C/R, dev-only)
Hot-reload, logs, capability inspection for locally loaded plugins. Only available in dev mode.
- **Use case:** plugin author workflow.

### `/_design-sandbox` — Design System Sandbox (internal)
Component gallery (primitives/charts/icons, theme toggle). Not a product surface.

---

## Surface map (quick index)

- **Read/analytics:** `/`, `/visibility`, `/competitors`, `/citations`, `/sentiment`, `/crawlers`, `/runs`, `/runs/[id]`, `/mcp`, `/mcp/[tool]`, `/marketplace(+/[slug])`, `/setup`
- **Control:** `/prompts`, `/schedules`, `/alerts`, `/audit`, `/recommendations(+/[id])`, `/settings`, `/setup/clickhouse/connect`, `/onboarding`, `/dev`
- **Evidence/reproducibility:** `/runs/[id]`, `/recommendations/[id]`
- **CLI-only affordances:** plugin install (`/marketplace`), bulk recommendation actions (`/recommendations`)
