# OpenGEO CLI Manual (`ogeo`)

The `ogeo` binary is the primary control plane: bootstrap a project, run prompts, schedule monitoring, gate CI, manage API/webhooks/plugins, and run audits. Source: `apps/cli/src` (clap-derived; `--help`/`--version` on every command).

**Conventions**
- Most commands resolve config from `--config <PATH>` (default `./opengeo.yaml`).
- Exit code `0` on success; on error the underlying `OpenGeoError.exit_code()` is used. CI-gating commands return non-zero deliberately (see each).
- Output formats are per-command: `table`/`json`, `human`/`json`/`markdown`, or `report`/`json`.

---

## 1. Project setup

### `ogeo init`
Scaffold a new project — writes `opengeo.yaml`, `.gitignore`, `README.md`.
- `--dir <DIR>` target (default cwd) · `--force` overwrite without prompting · `--no-overwrite` refuse overwrite & exit non-zero if any file exists (CI-safe).
- **Use case:** start a project; `--no-overwrite` for non-interactive provisioning.

### `ogeo login <provider>`
Capture & persist a provider API key into the secret store.
- `provider` ∈ `openai | anthropic | gemini | perplexity | grok | mistral | openrouter` (or a plugin provider). Key read interactively.
- Each maps to an env var fallback (`OPENAI_API_KEY`, …); missing-key errors name the var or this command.
- **Use case:** authenticate a provider before running prompts.

---

## 2. Prompts — `ogeo prompt`

| Command | Purpose | Key flags |
|---|---|---|
| `ogeo prompt add` | Add a tracked Prompt to YAML | `--name` (slug) `--text` `--description` `--config` (interactive if omitted) |
| `ogeo prompt list` | List declared prompts | `--format table\|json` |
| `ogeo prompt run` | Execute Prompts × Providers, persist runs | `--prompt <NAME>` (repeat) `--provider <NAME>` (repeat) `--use-mock-provider` (deterministic) |

- **Use cases:** declare what to track; `prompt run` is the core data-collection command. `--use-mock-provider` gives deterministic canned responses for smoke tests / screenshots.

---

## 3. Reporting & CI checks

### `ogeo report generate`
Summary report over a recent window.
- `--format human|json|markdown` (default human) · `--window 24h|7d|30d` (default 7d; accepts s/m/h/d).
- **Use case:** periodic visibility summary — markdown for PRs/docs, json for tooling.

### `ogeo check visibility`  *(CI gate)*
Assert a brand's ranking stays at/below a threshold.
- `--prompt <SLUG>` `--brand <NAME>` (must match `brand.name`) `--expect-rank-lte <N>` `--no-run` (check persisted data only).
- **Exit:** designed to exit **2** when all providers errored / nothing evaluable (hard CI failure).
- **Use case:** **visibility-as-code** — fail CI when brand ranking regresses (pairs with the `opengeo/check-visibility@v1` GitHub Action).

---

## 4. Dashboard

### `ogeo dashboard open`
Open or print the local dashboard URL.
- `--url <URL>` (default `OGEO_DASHBOARD_URL` or `http://127.0.0.1:5173`) · `--print` (headless/CI — print instead of launch).

---

## 5. Database — `ogeo db`

| Command | Purpose | Flags |
|---|---|---|
| `ogeo db backup` | Portable `pg_dump` archive | `--output <PATH>` (default `opengeo-backup-<date>.sql.gz`) `--database-url` |
| `ogeo db restore` | Restore a backup | backup file arg · `--database-url` |

- **Use case:** local backup/restore / migrate between machines.

---

## 6. Scheduling — `ogeo schedule`

| Command | Purpose | Key flags |
|---|---|---|
| `ogeo schedule add` | Declare a recurring run | `--name` `--cron` (cron or `hourly\|daily\|weekly\|every N minutes\|every N hours`) `--prompt` (repeat) `--provider` (repeat) `--debounce-minutes <N>` `--allow-expensive` (exceed monthly cost cap) |
| `ogeo schedule list` | List schedules | `--format table\|json` |
| `ogeo schedule remove` | Remove a schedule | `--name` |

- **Use case:** automate recurring monitoring with **cost guardrails** (projected monthly cost is capped unless `--allow-expensive`).

### `ogeo worker status`
Print background worker status (the process that executes scheduled runs).

---

## 7. REST API keys — `ogeo api key`  *(Phase 2)*

| Command | Purpose | Flags |
|---|---|---|
| `ogeo api key create` | Generate a key (**plaintext shown once**) | `--name` |
| `ogeo api key list` | List keys | `--active-only` (hide revoked) |
| `ogeo api key revoke` | Revoke by name | `--name` `--reason` |

- **Use case:** manage programmatic access to the `/v1` REST API (the same API the web app and MCP server use). Pipe run-level data into BI/warehouse via the REST surface using these keys.

---

## 8. Webhooks — `ogeo webhook`  *(Phase 2)*

| Command | Purpose | Key flags |
|---|---|---|
| `ogeo webhook add` | Declare a target (prints a fresh secret) | `--name` `--target-url` (HTTPS) `--event-kinds` (csv) |
| `ogeo webhook list` | List webhooks | `--active-only` |
| `ogeo webhook rotate-secret` | New secret (old stops working) | `--name` |
| `ogeo webhook reenable` | Re-enable after auto-disable | `--name` |

- **Event kinds:** `prompt_run.completed`, `visibility.regression`, `schedule.missed`, `visibility.anomaly`, `citation.anomaly`.
- Delivery: HMAC-SHA256 signed, ≤30s p95, retried (1s/30s/5m/1h/6h), auto-disabled after 5 permanent failures.
- **Use case:** push events into Slack/PagerDuty/your own handler — alerting like Alertmanager.

---

## 9. Public benchmark — `ogeo benchmark`  *(Phase 2)*

| Command | Purpose | Flags |
|---|---|---|
| `ogeo benchmark optin` | Opt project into the public dataset | `--yes` (skip terms prompt) `--actor` `--note` |
| `ogeo benchmark optout` | Stop future contributions | `--actor` `--note` |
| `ogeo benchmark status` | Show consent state | — |

- **Use case:** contribute to / withdraw from the anonymized public benchmark dataset (data-sharing is opt-in and audited).

---

## 10. Analytics backend — `ogeo analytics`  *(Phase 2)*

### `ogeo analytics migrate-to-clickhouse`
Migrate Postgres analytics into ClickHouse pre-aggregated tables (idempotent).
- `--days <N>` (1–365) rolling window per prompt · `--citation-limit <N>` (1–500) top-N domains.
- **Use case:** move analytics to ClickHouse for high-volume scale.

---

## 11. Plugins — `ogeo plugin`  *(Phase 3)*

| Command | Purpose | Key flags |
|---|---|---|
| `ogeo plugin validate <path>` | Validate a manifest YAML (no load/verify) | — |
| `ogeo plugin search <query>` | Search the registry index | `--registry <DIR>` |
| `ogeo plugin install <ns/name[@ver]>` | Download, verify signature, install | `--registry` `--allow-unsigned` |
| `ogeo plugin list` | List installed plugins | — |
| `ogeo plugin remove <id>` | Remove a plugin | — |
| `ogeo plugin upgrade <ns/name[@ver]>` | Upgrade | `--registry` `--allow-unsigned` `--accept-new-capabilities` |

- **Use case:** extend the platform (providers, trend kinds) via the plugin SDK / registry. Capability widening on upgrade requires explicit `--accept-new-capabilities`.

---

## 12. MCP server — `ogeo mcp`  *(Phase 3)*

| Command | Purpose | Key flags |
|---|---|---|
| `ogeo mcp serve` | Start the MCP server | `--transport stdio\|http+sse` `--bind <addr>` (default `127.0.0.1:7071`) `--require-api-key` |
| `ogeo mcp status` | Probe a running server | `--url` |
| `ogeo mcp install-config [client]` | Write an MCP config snippet | `client` = `claude-desktop`(default)\|`cursor`\|`zed` · `--config-path` · `--api-key` (or `OPENGEO_API_KEY`) |

- **Use case:** expose OpenGEO to AI assistants. `install-config` writes the editor/desktop snippet for you. See the [MCP manual](./mcp.md).

---

## 13. AI-crawler observability & audit

### `ogeo crawlers`
Verified AI-bot frequency, pages crawled, trends.
- `--days <N>` (default 30) · `--include-unverified` · `--format table|json` · `--ratio` (show crawl-to-refer ratio instead).
- **Use case:** see which AI crawlers (GPTBot, ClaudeBot, …) hit your site; `--ratio` shows whether crawls convert to referrals (degrades to `crawls_only` until referral attribution exists).

### `ogeo audit <target>`  *(Epic 32 — CI gate)*
Crawl owned pages and score citation-readiness (Identity / Extractability / Corroboration heuristics — open, in-tree).
- `target` = URL, sitemap URL, `file://`, or local HTML fixture.
- `--format report|json` · `--max-pages <N>` (default 25) · `--timeout-ms <N>` (default 10000) · `--fail-on <id|low|medium|high>` (repeatable/csv).
- **Use case:** **audit-as-code** — score your pages for LLM citation-readiness; gate CI on rule id or severity via `--fail-on`.

---

## CI recipes

**Fail a build if brand ranking regresses:**
```yaml
- uses: opengeo/check-visibility@v1
  with: { prompt: best-running-shoes, brand: Acme, expect-rank-lte: 3 }
# or raw:  ogeo check visibility --prompt best-running-shoes --brand Acme --expect-rank-lte 3
```

**Fail a build if a page isn't citation-ready:**
```bash
ogeo audit https://example.com --fail-on high --max-pages 50
```

**Export run data for BI (json → your pipeline):**
```bash
ogeo report generate --format json --window 30d | your-loader
```
