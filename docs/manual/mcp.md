# Anseo MCP Manual (`anseo-mcp`)

The MCP server exposes Anseo to AI agents/assistants (Claude Desktop, Cursor, Zed, Cline, …) over the [Model Context Protocol](https://modelcontextprotocol.io). It is the agent-native surface — same data and operations as the CLI/web, callable by an LLM.

- **Binary/crate:** `anseo-mcp` (`apps/mcp`). Launch via `anseo mcp serve` or directly; inspect the catalog with `anseo mcp tools`.
- **Protocol:** hand-rolled JSON-RPC 2.0, MCP `protocolVersion 2024-11-05`, `serverInfo.name = "anseo-mcp"`.
- **Process model:** the server **never touches storage directly** — every tool proxies over loopback HTTP to the local `/v1` REST API. (Exception: `search_benchmarks` hits the public benchmark service.) This is what guarantees CLI ⇄ Web ⇄ MCP parity.
- **Capabilities:** `tools` + `logging` only. **No MCP `resources` or `prompts` are exposed** (`initialize` returns them `null`).

---

## Launching

```bash
anseo mcp serve --transport stdio              # default; for desktop clients
anseo mcp serve --transport http+sse --bind 127.0.0.1:7071 --allow-public  # requires ANSEO_API_KEY
```

**Transports**
- **stdio** (default): line-delimited JSON, one message per line; logs to **stderr only** (stdout is the protocol channel). Best for Claude Desktop / Cursor / Zed.
- **http+sse**: `POST /mcp` (request/response), `GET /mcp/sse` (presence/keepalive — no server-push tools yet). Concurrency cap 32 in-flight (`429` beyond). With `--allow-public` (requires `ANSEO_API_KEY`), header `X-Anseo-API-Key` must equal `ANSEO_API_KEY` (else `401`); the server refuses to start public without a key set.

**Environment**
| Var | Default | Purpose |
|---|---|---|
| `ANSEO_API_URL` | `http://127.0.0.1:8080` | local `/v1` base |
| `ANSEO_API_KEY` | — | forwarded as `Authorization: Bearer`; required for public HTTP |
| `ANSEO_PROJECT_ID` | `default` | forwarded as `X-Anseo-Project` (actual project scoping) |
| `ANSEO_BENCHMARK_URL` | `https://benchmark.anseo.ai` | used only by `search_benchmarks` |

**Auth & scoping:** every loopback call forwards `Authorization: Bearer` + `X-Anseo-Project`. Most tools also take a `project` argument (the LLM-facing contract), but the server-level project header is what actually scopes data. `search_benchmarks` and `list_suite_prompts` are the deliberate project-less exceptions.

---

## Connecting a client

`anseo mcp install-config claude-desktop` writes the snippet for you. Equivalent stdio config:

```json
{
  "mcpServers": {
    "anseo": {
      "command": "anseo-mcp",
      "args": ["--transport", "stdio"],
      "env": {
        "ANSEO_API_URL": "http://127.0.0.1:8080",
        "ANSEO_API_KEY": "<key>",
        "ANSEO_PROJECT_ID": "default"
      }
    }
  }
}
```

---

## The tool set (closed — 16 tools)

The registry is a **closed set**: there is no tool-registration API, so plugins cannot add tools. Plugins surface namespaced values through existing tools (e.g. a plugin trend appears as `trend_kind = plugin:<name>:<kind>` via `list_trends`). All typed inputs use `deny_unknown_fields`; `search_benchmarks` and `list_suite_prompts` are the project-less exceptions. The shared `window` enum serializes as `"7d" | "30d" | "all"`. Every response embeds a ULID `trace_id` correlating to `/v1` logs.

### Read / analyze

| Tool | Backs | Input | Returns / use case |
|---|---|---|---|
| **`get_visibility`** | `GET /v1/visibility/trend` | `project`, `prompts?` (default `["default"]`), `window?` (30d) | Visibility trend series per prompt: points {date, provider, score, ranking, mention_count} + summary {latest, delta vs prior window}. *"How is my visibility trending?"* |
| **`compare_brands`** | `GET /v1/comparisons` | `project`, `window?` (7d), `prompts?` | Deterministic matrix: brand vs declared competitors across prompts/providers (ranking + mention_count per cell). *"How do I stack up vs competitors?"* |
| **`get_citations`** | `GET /v1/citations/summary` | `project`, `window?` (30d), `top_n?` (50, max 500) | Top cited domains: {domain, frequency, source_type, sample run ids}. *"Which sources do LLMs cite about us?"* |
| **`list_trends`** | `GET /v1/anomalies` | `project`, `window` (req), `min_significance?` (0.3) | Regressions / statistical anomalies / response-change trends with evidence run ids + significance. *"What changed / what should I worry about?"* |
| **`search_benchmarks`** | public benchmark service | `query` (req), `provider?`, `time_window?` (30d) | Category findings from the public dataset. **Only project-less tool** — sends just the query (privacy floor). *"What's normal for my industry?"* |
| **`list_suite_prompts`** | `GET /v1/suite/prompts` | — | Canonical benchmark prompt slugs `{slug, version, description}` for instrumentation parity. **Project-less global metadata**. *"Which prompt slugs should we reuse so our runs join the shared benchmark cohorts?"* |

### Act

| Tool | Backs | Input | Returns / use case |
|---|---|---|---|
| **`run_prompt`** | `POST /v1/prompt-runs` | `project`, `prompt` (name/ULID, req), `providers?` (default `["mock"]`), `idempotency_key?` | Per-provider results {status, ranking, mentions, citations, duration}; flagged `non_deterministic_pipeline: true`. *"Probe my visibility live right now."* |
| **`audit`** | `POST /v1/audit` | `target` (URL/sitemap/`file://`, req), `max_pages?` (25, 1–200), `fail_on?: string[]` | overall_score, pages_crawled, non-pass findings {page, rule_id, category, severity}, `gate_passed?` (only when fail_on given). Same engine as `anseo audit`. *"Audit my pages for citation-readiness."* |

### Recommendations lifecycle (`recommend.*`)
Each item is the engine wire envelope passed through **verbatim** (tags, `reproducibility.class`, `non_deterministic_pipeline` reach the LLM unchanged); every description carries the best-effort/non-deterministic caveat.

| Tool | Backs | Input | Use case |
|---|---|---|---|
| **`recommend.list`** | `GET /v1/recommendations` | `project`, `limit?` (50, max 200), `cursor?` | Page active recommendations, newest first. |
| **`recommend.show`** | `GET /v1/recommendations/{id}` | `project`, `recommendation_id` | Full recommendation incl. traceability + reproducibility. |
| **`recommend.ack`** | `PATCH .../state` | `project`, `recommendation_id` | Surfaced → Acknowledged. |
| **`recommend.dismiss`** | `PATCH .../state` | `project`, `recommendation_id` | Dismiss (from Surfaced/Acknowledged). |
| **`recommend.mark_acted`** | `PATCH .../state` | `project`, `recommendation_id`, `evidence_url?`, `note?` | Acknowledged → Acted; missing evidence returns a `lifecycle.evidence_missing` warning. |

---

## JSON-RPC reference

Methods: `initialize`, `initialized`/`notifications/initialized` (silent), `tools/list`, `tools/call` (`{name, arguments}`), `ping`. Error codes: `-32700` parse, `-32600` invalid request, `-32601` method/tool not found, `-32602` invalid params, `-32603` internal (tool upstream failures map here, with `data.upstream` carrying the `McpError` → embedded REST `ApiError`).

`tools/call` errors degrade sensibly: e.g. `compare_brands` with placeholder/unconfigured brands returns an **empty matrix** rather than erroring; `search_benchmarks` network failure → `upstream_unreachable`.

---

## Agent use-case recipes

- **"Is my brand losing ground this week?"** → `compare_brands(window:7d)` + `list_trends(window:7d)`.
- **"Why aren't we cited for X?"** → `get_citations` (see who *is* cited) then `audit(target: our-page)` (find readiness gaps), then `recommend.list`.
- **"Check our visibility right now for this prompt."** → `run_prompt(prompt, providers)` then `get_visibility`.
- **"Which slug should my external instrumentation emit?"** → `list_suite_prompts`.
- **"Triage and close recommendations."** → `recommend.list` → `recommend.show` → `recommend.mark_acted(evidence_url)`.

## Monitoring the server (web)
`/mcp` lists the registered tools + recent calls; `/mcp/[tool]` shows per-tool call count, error rate, p50/p95 latency, and invocation history. See the [Web manual](./web.md).
