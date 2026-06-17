# Anseo CLI Manual (`anseo`)

The `anseo` binary is the primary control plane: bootstrap a project, run prompts, schedule monitoring, gate CI, manage API/webhooks/plugins, and run audits. Source: `apps/cli/src` (clap-derived; `--help`/`--version` on every command). (`ogeo` remains a deprecated alias for the `anseo` CLI.)

**Conventions**
- Most commands resolve config from `--config <PATH>` (default `./anseo.yaml`).
- Exit code `0` on success; on error the underlying `OpenGeoError.exit_code()` is used. CI-gating commands return non-zero deliberately (see each).
- Output formats are per-command: `table`/`json`, `human`/`json`/`markdown`, or `report`/`json`.

---

## 1. Project setup

### `anseo init`
Scaffold a new project — writes `anseo.yaml`, `.gitignore`, `README.md`.
- `--dir <DIR>` target (default cwd) · `--force` overwrite without prompting · `--no-overwrite` refuse overwrite & exit non-zero if any file exists (CI-safe).
- **Use case:** start a project; `--no-overwrite` for non-interactive provisioning.

### `anseo login <provider>`
Capture & persist a provider API key into the secret store.
- `provider` ∈ `openai | anthropic | gemini | perplexity | grok | mistral | openrouter` (or a plugin provider). Key read interactively.
- Each maps to an env var fallback (`OPENAI_API_KEY`, …); missing-key errors name the var or this command.
- **Use case:** authenticate a provider before running prompts.

---

## 2. Prompts — `anseo prompt`

| Command | Purpose | Key flags |
|---|---|---|
| `anseo prompt add` | Add a tracked Prompt to YAML | `--name` (slug) `--text` `--description` `--config` (interactive if omitted) |
| `anseo prompt list` | List declared prompts | `--format table\|json` |
| `anseo prompt run` | Execute Prompts × Providers, persist runs | `--prompt <NAME>` (repeat) `--provider <NAME>` (repeat) `--use-mock-provider` (deterministic) |

- **Use cases:** declare what to track; `prompt run` is the core data-collection command. `--use-mock-provider` gives deterministic canned responses for smoke tests / screenshots.

---

## 3. Reporting & CI checks

### `anseo report generate`
Summary report over a recent window.
- `--format human|json|markdown` (default human) · `--window 24h|7d|30d` (default 7d; accepts s/m/h/d).
- **Use case:** periodic visibility summary — markdown for PRs/docs, json for tooling.

### `anseo check visibility`  *(CI gate)*
Assert a brand's ranking stays at/below a threshold.
- `--prompt <SLUG>` `--brand <NAME>` (must match `brand.name`) `--expect-rank-lte <N>` `--no-run` (check persisted data only).
- **Exit:** designed to exit **2** when all providers errored / nothing evaluable (hard CI failure).
- **Use case:** **visibility-as-code** — fail CI when brand ranking regresses (pairs with the `leandropalazzo/anseo/infra/github-action@v1` GitHub Action).

---

## 4. Dashboard

### `anseo dashboard open`
Open or print the local dashboard URL.
- `--url <URL>` (default `ANSEO_DASHBOARD_URL` or `http://127.0.0.1:5173`) · `--print` (headless/CI — print instead of launch).

---

## 5. Database — `anseo db`

| Command | Purpose | Flags |
|---|---|---|
| `anseo db backup` | Portable `pg_dump` archive | `--output <PATH>` (default `anseo-backup-<date>.sql.gz`) `--database-url` |
| `anseo db restore` | Restore a backup | backup file arg · `--database-url` |

- **Use case:** local backup/restore / migrate between machines.

---

## 6. Scheduling — `anseo schedule`

| Command | Purpose | Key flags |
|---|---|---|
| `anseo schedule add` | Declare a recurring run | `--name` `--cron` (cron or `hourly\|daily\|weekly\|every N minutes\|every N hours`) `--prompt` (repeat) `--provider` (repeat) `--debounce-minutes <N>` `--allow-expensive` (exceed monthly cost cap) |
| `anseo schedule list` | List schedules | `--format table\|json` |
| `anseo schedule remove` | Remove a schedule | `--name` |

- **Use case:** automate recurring monitoring with **cost guardrails** (projected monthly cost is capped unless `--allow-expensive`).

### `anseo worker status`
Print background worker status (the process that executes scheduled runs).

---

## 7. REST API keys — `anseo api key`  *(Phase 2)*

| Command | Purpose | Flags |
|---|---|---|
| `anseo api key create` | Generate a key (**plaintext shown once**) | `--name` |
| `anseo api key list` | List keys | `--active-only` (hide revoked) |
| `anseo api key revoke` | Revoke by name | `--name` `--reason` |

- **Use case:** manage programmatic access to the `/v1` REST API (the same API the web app and MCP server use). Pipe run-level data into BI/warehouse via the REST surface using these keys.

---

## 8. Webhooks — `anseo webhook`  *(Phase 2)*

| Command | Purpose | Key flags |
|---|---|---|
| `anseo webhook add` | Declare a target (prints a fresh secret) | `--name` `--target-url` (HTTPS) `--event-kinds` (csv) |
| `anseo webhook list` | List webhooks | `--active-only` |
| `anseo webhook rotate-secret` | New secret (old stops working) | `--name` |
| `anseo webhook reenable` | Re-enable after auto-disable | `--name` |

- **Event kinds:** `prompt_run.completed`, `visibility.regression`, `schedule.missed`, `visibility.anomaly`, `citation.anomaly`.
- Delivery: HMAC-SHA256 signed, ≤30s p95, retried (1s/30s/5m/1h/6h), auto-disabled after 5 permanent failures.
- **Use case:** push events into Slack/PagerDuty/your own handler — alerting like Alertmanager.

---

## 9. Public benchmark — `anseo benchmark`  *(Phase 2)*

| Command | Purpose | Flags |
|---|---|---|
| `anseo benchmark optin` | Opt project into the public dataset | `--yes` (skip terms prompt) `--actor` `--note` |
| `anseo benchmark optout` | Stop future contributions | `--actor` `--note` |
| `anseo benchmark status` | Show consent state | — |

- **Use case:** contribute to / withdraw from the anonymized public benchmark dataset (data-sharing is opt-in and audited).

---

## 10. Analytics backend — `anseo analytics`  *(Phase 2)*

### `anseo analytics migrate-to-clickhouse`
Migrate Postgres analytics into ClickHouse pre-aggregated tables (idempotent).
- `--days <N>` (1–365) rolling window per prompt · `--citation-limit <N>` (1–500) top-N domains.
- **Use case:** move analytics to ClickHouse for high-volume scale.

---

## 11. Plugins — `anseo plugin`  *(Phase 3)*

| Command | Purpose | Key flags |
|---|---|---|
| `anseo plugin validate <path>` | Validate a manifest YAML (no load/verify) | — |
| `anseo plugin search <query>` | Search the registry index | `--registry <DIR>` `--refresh` |
| `anseo plugin install <ns/name[@ver]>` | Download, verify signature, install | `--registry` `--allow-unsigned` |
| `anseo plugin list` | List installed plugins | — |
| `anseo plugin remove <id>` | Remove a plugin | — |
| `anseo plugin upgrade <ns/name[@ver]>` | Upgrade | `--registry` `--allow-unsigned` `--accept-new-capabilities` |

- **Use case:** extend the platform (providers, trend kinds) via the plugin SDK / registry. By default, search reads the live GitHub registry at `leandropalazzo/plugin-registry` and caches `index.toml` for up to one hour; `--refresh` forces a re-check. Capability widening on upgrade requires explicit `--accept-new-capabilities`.
- **Surface boundary:** plugins reach users through *existing* surfaces only (the `plugin:<id>:<kind>` namespace) — they cannot mint new MCP tools, Web routes, or CLI verbs. See the [Plugin Surface Boundary](../plugin-surface-boundary.md) for the load-path gates, signing requirements, and the one accepted parity exception.

---

## 12. MCP server — `anseo mcp`  *(Phase 3)*

| Command | Purpose | Key flags |
|---|---|---|
| `anseo mcp serve` | Start the MCP server | `--transport stdio\|http+sse` `--bind <addr>` (default `127.0.0.1:7071`) `--allow-public` |
| `anseo mcp status` | Probe a running server | `--url` |
| `anseo mcp install-config [client]` | Write an MCP config snippet | `client` = `claude-desktop`(default)\|`cursor`\|`zed` · `--config-path` · `--api-key` (or `ANSEO_API_KEY`) |

- **Use case:** expose Anseo to AI assistants. `install-config` writes the editor/desktop snippet for you. See the [MCP manual](./mcp.md).

---

## 13. AI-crawler observability & audit

### `anseo crawlers`
Verified AI-bot frequency, pages crawled, trends.
- `--days <N>` (default 30) · `--include-unverified` · `--format table|json` · `--ratio` (show crawl-to-refer ratio instead).
- **Use case:** see which AI crawlers (GPTBot, ClaudeBot, …) hit your site; `--ratio` shows whether crawls convert to referrals (degrades to `crawls_only` until referral attribution exists).

### `anseo audit <target>`  *(Epic 32 — CI gate)*
Crawl owned pages and score citation-readiness (Identity / Extractability / Corroboration heuristics — open, in-tree).
- `target` = URL, sitemap URL, `file://`, or local HTML fixture.
- `--format report|json` · `--max-pages <N>` (default 25) · `--timeout-ms <N>` (default 10000) · `--fail-on <id|low|medium|high>` (repeatable/csv).
- **Use case:** **audit-as-code** — score your pages for LLM citation-readiness; gate CI on rule id or severity via `--fail-on`.

---

## CI recipes

**Fail a build if brand ranking regresses:**
```yaml
- uses: leandropalazzo/anseo/infra/github-action@v1
  with: { prompt: best-running-shoes, brand: Acme, expect-rank-lte: 3 }
# or raw:  anseo check visibility --prompt best-running-shoes --brand Acme --expect-rank-lte 3
```

**Fail a build if a page isn't citation-ready:**
```bash
anseo audit https://example.com --fail-on high --max-pages 50
```

**Export run data for BI (json → your pipeline):**
```bash
anseo report generate --format json --window 30d | your-loader
```

---

## Instrumentation SDKs

To feed **externally-executed** LLM runs into Anseo from your own application
(the OpenTelemetry pattern), use the thin instrumentation SDKs in
[`packages/`](../../packages/) — Python (`sdk-python`, `anseo_observe`),
TypeScript (`sdk-typescript`, `@anseo/observe`), and Go (`sdk-go`). They wrap a
call and POST the run to `/v1/ingest/run`, best-effort. Full usage docs:
documented separately (Story 40.5).
