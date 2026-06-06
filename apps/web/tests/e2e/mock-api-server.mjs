// Test-time mock API backend for the Playwright E2E suite.
//
// The dashboard pages are Next.js server components that fetch the OpenGEO API
// during SSR (e.g. `await fetchMcpTools()` -> GET ${OGEO_API_BASE_URL}/v1/mcp/tools).
// Playwright `page.route()` only intercepts *browser* requests, never the
// server-side SSR fetches, so with no backend those fetches fail and the pages
// render empty/EmptyState. This zero-dependency Node http server stands in for
// the real API during the Playwright run: `playwright.config.ts` starts it and
// points the dev server's OGEO_API_BASE_URL at it so SSR fetches return canned
// data that satisfies the specs' assertions.
//
// Response shapes mirror `apps/web/lib/api/*.ts` 1:1. Mock data includes every
// id/entity the specs assert (tool ids get_visibility / run_prompt, the api-key
// providers openai/anthropic/google, an NDP recommendation, etc.).
//
// Pure stdlib (node:http + node:url). NOT TypeScript — excluded from tsc/lint.

import http from "node:http";
import { URL } from "node:url";

const PORT = Number(process.env.MOCK_API_PORT ?? 8787);

// ─── Fixtures ────────────────────────────────────────────────────────────────

// /v1/mcp/tools -> { tools: McpToolInfo[] } — ids asserted by mcp-dashboard.spec
const MCP_TOOLS = {
  tools: [
    {
      id: "get_visibility",
      sig: "(prompt: string, days?: number)",
      doc: "Get visibility scores for a prompt",
      category: "visibility",
    },
    {
      id: "run_prompt",
      sig: "(prompt: string)",
      doc: "Run a prompt across configured providers",
      category: "runs",
    },
    {
      id: "list_citations",
      sig: "(domain?: string)",
      doc: "List citation domains observed across runs",
      category: "analytics",
    },
  ],
};

// /v1/mcp/calls -> { calls: McpCallRow[] } — empty so ActivityLog shows its
// "ogeo mcp serve" empty state (mcp-dashboard.spec).
const MCP_CALLS = { calls: [] };

// /v1/mcp/stats -> McpToolStats
const MCP_STATS = {
  tool_name: "get_visibility",
  total_calls: 0,
  ok_calls: 0,
  error_calls: 0,
  error_rate: 0,
  p50_ms: null,
  p95_ms: null,
};

// /v1/recommendations -> { items: Recommendation[], next_cursor }
// Include one byte-stable + one non-deterministic (NDP) rec so the list renders
// the rec-ndp-marker and the detail evidence chips have real run/citation ids.
const REC_ID_NDP = "01JABCDEF0123456789ABCDEFG";
function makeRec(overrides) {
  return {
    id: "01JREC000000000000000000A1",
    project_id: "demo",
    kind: "improve_citation_coverage",
    severity: "high",
    confidence_band: "high",
    state: "surfaced",
    summary: "Citation coverage for 'best crm software' dropped on anthropic.",
    payload: { metric: "presence_rate", delta: -0.18 },
    traceability: {
      source_run_ids: ["01JRUN0000000000000000OK01", "01JRUN0000000000000000OK02"],
      source_run_ids_truncated: false,
      source_citation_ids: ["01JCIT0000000000000000AA01"],
      source_citation_ids_truncated: false,
      source_benchmark_queries: [
        { name: "best crm software", query_hash: "qh_0001" },
      ],
      window: { start: "2026-05-24T00:00:00Z", end: "2026-05-31T00:00:00Z" },
      input_fingerprint: "fp_deadbeef",
    },
    reproducibility: { class: "byte_stable", note: null },
    tags: ["citation"],
    generated_at: "2026-05-30T12:00:00Z",
    engine_version: "1.2.3",
    ...overrides,
  };
}

const RECOMMENDATIONS = {
  items: [
    makeRec({}),
    makeRec({
      id: REC_ID_NDP,
      severity: "medium",
      kind: "non_deterministic_ranking",
      summary: "Ranking volatility detected on a non-deterministic pipeline.",
      reproducibility: {
        class: "non_deterministic",
        note: "Provider responses are non-reproducible run-to-run.",
      },
      tags: ["non_deterministic_pipeline"],
    }),
    makeRec({
      id: "01JREC000000000000000000C3",
      severity: "low",
      kind: "tune_schedule",
      summary: "A low-traffic prompt is scheduled hourly; consider daily.",
    }),
  ],
  next_cursor: null,
};

// Detail responses by id. The asserted REC_ID has full traceability so the
// detail page renders rec-evidence (run hrefs) + rec-action-snooze/dismiss.
function recDetailFor(id) {
  const found = RECOMMENDATIONS.items.find((r) => r.id === id);
  if (found) return found;
  // Unknown id: still return a valid, populated rec (never 404) so the detail
  // page renders rather than calling notFound().
  return makeRec({ id });
}

// /api/runs -> { runs: RunListRow[] } — 2 ok + 2 failed (provider_rate_limited)
// to satisfy runs-partial-failure.spec assertions.
const RUNS = {
  runs: [
    {
      id: "01JRUN0000000000000000OK01",
      prompt_name: "p1",
      provider: "openai",
      provider_model_version: "gpt-4o-2024-08-06",
      started_at: "2026-05-31T09:00:00Z",
      status: "ok",
      error_kind: null,
    },
    {
      id: "01JRUN0000000000000000OK02",
      prompt_name: "p2",
      provider: "openai",
      provider_model_version: "gpt-4o-2024-08-06",
      started_at: "2026-05-31T09:01:00Z",
      status: "ok",
      error_kind: null,
    },
    {
      id: "01JRUN000000000000000FAIL1",
      prompt_name: "p1",
      provider: "anthropic",
      provider_model_version: "claude-3-5-sonnet",
      started_at: "2026-05-31T09:02:00Z",
      status: "failed",
      error_kind: "provider_rate_limited",
    },
    {
      id: "01JRUN000000000000000FAIL2",
      prompt_name: "p2",
      provider: "anthropic",
      provider_model_version: "claude-3-5-sonnet",
      started_at: "2026-05-31T09:03:00Z",
      status: "failed",
      error_kind: "provider_rate_limited",
    },
  ],
};

function runDetailFor(id) {
  const row =
    RUNS.runs.find((r) => r.id === id) ?? RUNS.runs[0];
  return {
    ...row,
    id,
    prompt_id: "01JPROMPT00000000000000001",
    finished_at: "2026-05-31T09:00:05Z",
    raw_response: { text: "mock raw response" },
    request_parameters: { temperature: 0 },
  };
}

// /api/runs/:id/mentions|citations|provenance|responses -> arrays
const RUN_MENTIONS = [
  {
    id: "01JMEN0000000000000000AA01",
    entity: "OpenGEO",
    provider: "openai",
    rank: 1,
    char_offset: 42,
    matched_text: "OpenGEO",
  },
];
const RUN_CITATIONS = [
  {
    id: "01JCIT0000000000000000AA01",
    domain: "example.com",
    url: "https://example.com/a",
    source_type: "web",
    frequency: 3,
    provider: "openai",
  },
];
const RUN_PROVENANCE = [];
const RUN_RESPONSES = [
  {
    provider: "openai",
    provider_model_version: "gpt-4o-2024-08-06",
    status: "ok",
    raw_response: { text: "mock raw response" },
  },
];

// /v1/prompts -> PromptView[]
const PROMPTS = [
  {
    id: "01JPROMPT00000000000000001",
    name: "best crm software",
    text: "Which CRM tools are best for AI-native GTM teams?",
    tags: ["crm"],
    created_at: "2026-05-01T00:00:00Z",
  },
  {
    id: "p_001",
    name: "best-vector-db",
    text: "best vector database for production RAG",
    tags: ["infra", "ai"],
    created_at: "2026-05-20T08:00:00Z",
  },
  {
    id: "p_002",
    name: "observability",
    text: "best observability platforms for startups",
    tags: ["devtools"],
    created_at: "2026-05-21T09:00:00Z",
  },
  {
    id: "p_003",
    name: "auth-saas",
    text: "best authentication SaaS for B2B apps",
    tags: [],
    created_at: "2026-05-22T10:00:00Z",
  },
];

// /api/citations/summary -> { domains: CitationSummaryRow[] }
const CITATION_SUMMARY = {
  domains: [
    { domain: "example.com", frequency: 12, source_type: "web" },
    { domain: "docs.example.org", frequency: 7, source_type: "docs" },
    { domain: "news.example.net", frequency: 3, source_type: "news" },
  ],
  citation_score: {
    score: 82,
    total_citations: 22,
    distinct_domains: 3,
    quality_share: 0.86,
    growth_rate: 0.18,
    volume_component: 32,
    diversity_component: 18,
    quality_component: 30,
  },
  window_days: 30,
};

// /api/citations/trend -> { trend: Record<domain, CitationTrendPoint[]> }
const CITATION_TREND = {
  trend: {
    "example.com": [
      { bucket_start: "2026-05-29T00:00:00Z", frequency: 2 },
      { bucket_start: "2026-05-30T00:00:00Z", frequency: 5 },
      { bucket_start: "2026-05-31T00:00:00Z", frequency: 7 },
    ],
    "docs.example.org": [
      { bucket_start: "2026-05-29T00:00:00Z", frequency: 1 },
      { bucket_start: "2026-05-30T00:00:00Z", frequency: 3 },
      { bucket_start: "2026-05-31T00:00:00Z", frequency: 3 },
    ],
  },
};

// /api/visibility/trend -> { points: VisibilityPoint[] }
const VISIBILITY_TREND = {
  points: [
    { bucket_start: "2026-05-29T00:00:00Z", provider: "openai", avg_rank: 2.0, presence_rate: 0.8 },
    { bucket_start: "2026-05-30T00:00:00Z", provider: "openai", avg_rank: 1.5, presence_rate: 0.9 },
    { bucket_start: "2026-05-31T00:00:00Z", provider: "anthropic", avg_rank: 3.0, presence_rate: 0.6 },
  ],
};

// /api/visibility/overall -> VisibilityOverall
const VISIBILITY_OVERALL = {
  brand: "OpenGEO",
  window_days: 7,
  matrix: [
    {
      prompt_name: "best crm software",
      provider: "openai",
      run_count: 14,
      mention_count: 12,
      presence_rate: 0.86,
      avg_rank: 1.4,
    },
    {
      prompt_name: "best crm software",
      provider: "perplexity",
      run_count: 8,
      mention_count: 6,
      presence_rate: 0.75,
      avg_rank: 2.1,
    },
  ],
  trend: VISIBILITY_TREND.points,
};

// /v1/schedules -> { schedules: ScheduleSummary[] }
const SCHEDULES = {
  schedules: [
    {
      id: "01JSCH0000000000000000AA01",
      name: "daily-crm",
      cron: "0 9 * * *",
      prompts: ["best crm software"],
      providers: ["openai", "anthropic"],
      debounce_minutes: 30,
      projected_monthly_usd: 12.5,
      projection_acknowledged_at: "2026-05-01T00:00:00Z",
      paused: false,
      created_at: "2026-05-01T00:00:00Z",
      last_tick_at: "2026-05-31T09:00:00Z",
      last_tick_status: "ok",
    },
  ],
};

// /v1/alert-rules -> { items: AlertRule[] }
const ALERT_RULES = {
  items: [
    {
      name: "visibility-drop",
      on: "presence_rate < 0.5",
      target: "*",
      channels: ["webhook"],
      status: "armed",
      fires: 2,
    },
    {
      name: "citation-loss",
      on: "citation_frequency drop > 20%",
      target: "best crm software",
      channels: ["webhook"],
      status: "muted",
      fires: 0,
    },
  ],
};

// /anomalies -> { items: AnomalyItem[], trace_id }
const ANOMALIES = {
  items: [
    {
      id: "01JANO0000000000000000AA01",
      kind: "visibility_drop",
      prompt: "best crm software",
      provider: "anthropic",
      detected_at: "2026-05-31T08:00:00Z",
      severity: "high",
      delta: -0.18,
      window_days: 7,
      details: { metric: "presence_rate" },
    },
  ],
  trace_id: "trace_mock_anomalies",
};

// /comparisons -> CompareBrandsOutput
const COMPARISONS = {
  window: "7d",
  brand: "OpenGEO",
  competitors: ["Acme", "Globex"],
  rows: [
    {
      prompt_id: "01JPROMPT00000000000000001",
      prompt_name: "best crm software",
      provider: "openai",
      cells: [
        { subject: "OpenGEO", ranking: 1, mention_count: 5 },
        { subject: "Acme", ranking: 2, mention_count: 3 },
        { subject: "Globex", mention_count: 1 },
      ],
    },
  ],
  trace_id: "trace_mock_comparisons",
};

// /brands -> { items: BrandItem[] }
const BRANDS = {
  items: [
    {
      name: "OpenGEO",
      is_primary: true,
      mention_count_7d: 42,
      avg_rank_7d: 1.4,
      providers_with_data: ["openai", "anthropic"],
    },
    {
      name: "Acme",
      is_primary: false,
      mention_count_7d: 18,
      avg_rank_7d: 2.6,
      providers_with_data: ["openai"],
    },
  ],
};

// /prompts/run-summary -> { items: RunSummaryItem[], since }
const RUN_SUMMARY = {
  items: [
    {
      prompt: "best crm software",
      run_count: 14,
      last_run_at: "2026-05-31T09:00:00Z",
      success_rate: 0.86,
      avg_latency_ms: 1200,
      providers: ["openai", "anthropic"],
    },
    {
      prompt: "top project management tools",
      run_count: 9,
      last_run_at: "2026-05-30T18:00:00Z",
      success_rate: 1,
      avg_latency_ms: 900,
      providers: ["openai"],
    },
  ],
  since: "2026-05-24T00:00:00Z",
};

// /v1/prompts/tag-summary -> { items: TagSummaryItem[], since }
const TAG_SUMMARY = {
  items: [
    {
      tag: "crm",
      prompt_count: 1,
      run_count: 14,
      success_rate: 0.86,
      providers: ["openai", "anthropic"],
    },
    {
      tag: "AUTO",
      prompt_count: 1,
      run_count: 9,
      success_rate: 1,
      providers: ["openai"],
    },
  ],
  since: "2026-05-24T00:00:00Z",
};

// /v1/prompts/kpi-trend -> { points: KpiTrendPoint[] }
const KPI_TREND = {
  points: [
    {
      bucket_start: "2026-05-29T00:00:00Z",
      run_count: 3,
      success_rate: 0.67,
      avg_latency_ms: 1300,
    },
    {
      bucket_start: "2026-05-30T00:00:00Z",
      run_count: 8,
      success_rate: 0.88,
      avg_latency_ms: 1100,
    },
    {
      bucket_start: "2026-05-31T00:00:00Z",
      run_count: 12,
      success_rate: 1,
      avg_latency_ms: 950,
    },
  ],
};

// /v1/analytics/citation-graph -> CitationGraph
const CITATION_GRAPH = {
  nodes: [
    { id: "provider:openai", kind: "provider", label: "openai" },
    { id: "domain:example.com", kind: "domain", label: "example.com" },
  ],
  edges: [{ source: "provider:openai", target: "domain:example.com", weight: 12 }],
};

// /v1/analytics/heatmap -> Heatmap
const HEATMAP = {
  cells: [
    { date: "2026-05-30", provider: "openai", runs: 4, presence_rate: 0.9, avg_rank: 1.5 },
    { date: "2026-05-31", provider: "anthropic", runs: 3, presence_rate: 0.6, avg_rank: 3.0 },
  ],
};

// /v1/analytics/volatility -> Volatility
const VOLATILITY = { value: 0.22, presence_ratio: 0.8, samples: 14 };

// /v1/setup/status -> SetupStatus
// api_keys must contain openai/anthropic/google so setup-keys-and-webhook.spec
// finds api-key-row-{openai,anthropic,google}; google is unconfigured so its
// revoke button is disabled.
const SETUP_STATUS = {
  postgres: {
    state: "healthy",
    schema_version: 42,
    row_count_estimate: 100000,
    last_write_at: "2026-05-31T09:00:00Z",
  },
  clickhouse: {
    state: "healthy",
    url: "http://clickhouse:8123",
    row_count: 250000,
    etl_lag_seconds: 3,
  },
  worker: { state: "running", uptime_seconds: 3600, queue_depth: 0 },
  webhook_target: {
    configured: true,
    last_delivery_at: "2026-05-01T11:00:00Z",
    last_status: "200",
  },
  api_keys: [
    { provider: "openai", configured: true, last_used_at: "2026-05-01T10:00:00Z" },
    { provider: "anthropic", configured: true, last_used_at: null },
    { provider: "google", configured: false, last_used_at: null },
  ],
  docker: { present: true, version: "24.0.7" },
};

// Empty variant served when the setup page forwards `?empty=1` (SSR can't be
// intercepted by page.route, so the empty-state spec drives it via this flag).
// No api keys + unconfigured webhook so ApiKeysCard renders "No API keys
// configured" and WebhookTargetCard shows its unconfigured state.
const SETUP_STATUS_EMPTY = {
  ...SETUP_STATUS,
  api_keys: [],
  webhook_target: { configured: false, last_delivery_at: null, last_status: null },
};

// /v1/setup/clickhouse/status -> ClickHouseEtlStatus
const ETL_STATUS = {
  state: "completed",
  batches_done: 10,
  batches_total: 10,
  last_heartbeat_at: "2026-05-31T09:00:00Z",
  finished_at: "2026-05-31T09:00:05Z",
  error: null,
};

// /v1/plugins -> { plugins: MarketplacePlugin[] } — slugs match marketplace-mock
function makePlugin(overrides) {
  return {
    slug: "opengeo/serp-enrichment",
    name: "SERP Enrichment 🚀",
    version: "1.4.2",
    description: "Enrich runs with SERP data.",
    author: "opengeo",
    homepage: "https://example.com/serp",
    plugin_type: "extractor",
    verified: true,
    signature_status: "signed",
    capabilities: [{ kind: "network", allowlist: ["serpapi.com"] }],
    installed: true,
    installed_version: "1.4.0",
    update_available: true,
    ...overrides,
  };
}
const PLUGINS = [
  makePlugin({}),
  makePlugin({
    slug: "community/markdown-export",
    name: "Markdown Export ✨",
    version: "0.9.0",
    description: "Export reports as Markdown.",
    author: "community",
    homepage: "https://example.com/md",
    plugin_type: "output-format",
    verified: false,
    signature_status: "unsigned",
    capabilities: [{ kind: "emit-event", kinds: ["report.exported"] }],
    installed: false,
    installed_version: undefined,
    update_available: false,
  }),
  makePlugin({
    slug: "opengeo/clickhouse-window",
    name: "ClickHouse Windowed Analytics",
    version: "2.0.1",
    description: "Windowed analytics over ClickHouse.",
    author: "opengeo",
    plugin_type: "analytics",
    verified: true,
    signature_status: "signed",
    capabilities: [{ kind: "analytics-window", windows: ["7d", "30d"] }],
    installed: true,
    installed_version: "2.0.1",
    update_available: false,
  }),
];
function pluginFor(slug) {
  return PLUGINS.find((p) => p.slug === slug) ?? null;
}

// /v1/projects -> { projects: ProjectView[] } (Story 36.8). Mutable so the
// E2E can create/archive and observe the list reflow. Two seed projects let
// the switcher test flip between them and assert the data changes.
const PROJECTS = [
  { project_id: "01JPROJ0000000000000000AA", name: "Acme", created_at: "2026-05-01T00:00:00Z" },
  { project_id: "01JPROJ0000000000000000BB", name: "Globex", created_at: "2026-05-02T00:00:00Z" },
];

// Per-project visibility so the switcher test can assert the data follows the
// selection. Keyed by the X-Anseo-Project header (brand name). Falls back to
// the default VISIBILITY_OVERALL when the header is absent/unknown.
const VISIBILITY_BY_PROJECT = {
  Acme: { ...VISIBILITY_OVERALL, brand: "Acme" },
  Globex: { ...VISIBILITY_OVERALL, brand: "Globex" },
};

let nextProjectSeq = 0;
function createProject(body) {
  let name = "New Project";
  try {
    const parsed = JSON.parse(body || "{}");
    if (parsed && typeof parsed.name === "string" && parsed.name.trim()) {
      name = parsed.name.trim();
    }
  } catch {
    /* keep default */
  }
  nextProjectSeq += 1;
  const project_id = `01JPROJNEW${String(nextProjectSeq).padStart(14, "0")}`;
  PROJECTS.push({ project_id, name, created_at: "2026-06-03T00:00:00Z" });
  return { project_id, name };
}
function archiveProject(id) {
  const idx = PROJECTS.findIndex((p) => p.project_id === id);
  if (idx >= 0) PROJECTS.splice(idx, 1);
}

// ─── Routing ─────────────────────────────────────────────────────────────────

const JSON_HEADERS = { "Content-Type": "application/json" };

function send(res, status, body) {
  res.writeHead(status, JSON_HEADERS);
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

/** Match GET routes. Returns the body, or undefined if no match. */
function routeGet(pathname, searchParams, projectHeader) {
  // Static exact matches first.
  switch (pathname) {
    case "/healthz":
      return { ok: true };
    case "/v1/mcp/tools":
      return MCP_TOOLS;
    case "/v1/mcp/calls":
      return MCP_CALLS;
    case "/v1/mcp/stats":
      return MCP_STATS;
    case "/v1/recommendations":
      return RECOMMENDATIONS;
    case "/api/runs":
      return RUNS;
    case "/api/citations/summary":
      return CITATION_SUMMARY;
    case "/api/citations/trend":
      return CITATION_TREND;
    case "/api/visibility/trend":
      return VISIBILITY_TREND;
    case "/api/visibility/overall":
      return (projectHeader && VISIBILITY_BY_PROJECT[projectHeader]) || VISIBILITY_OVERALL;
    case "/v1/schedules":
      return SCHEDULES;
    case "/v1/prompts":
      return PROMPTS;
    case "/v1/alert-rules":
      return ALERT_RULES;
    case "/anomalies":
      return ANOMALIES;
    case "/comparisons":
      return COMPARISONS;
    case "/v1/comparisons":
      return COMPARISONS;
    case "/brands":
      return BRANDS;
    case "/v1/brands":
      return BRANDS;
    case "/prompts/run-summary":
      return RUN_SUMMARY;
    case "/v1/prompts/run-summary":
      return RUN_SUMMARY;
    case "/v1/prompts/tag-summary":
      return TAG_SUMMARY;
    case "/v1/prompts/kpi-trend":
      return KPI_TREND;
    case "/v1/analytics/citation-graph":
      return CITATION_GRAPH;
    case "/v1/analytics/heatmap":
      return HEATMAP;
    case "/v1/analytics/volatility":
      return VOLATILITY;
    case "/v1/setup/status":
      return searchParams?.get("empty") === "1" ? SETUP_STATUS_EMPTY : SETUP_STATUS;
    case "/v1/setup/clickhouse/status":
      return ETL_STATUS;
    case "/v1/plugins":
      return { plugins: PLUGINS };
    case "/v1/projects":
      return { projects: PROJECTS };
    default:
      break;
  }

  // Parameterised routes.
  let m;
  if ((m = pathname.match(/^\/v1\/recommendations\/([^/]+)$/))) {
    return recDetailFor(decodeURIComponent(m[1]));
  }
  if ((m = pathname.match(/^\/api\/runs\/([^/]+)\/mentions$/))) {
    void m;
    return RUN_MENTIONS;
  }
  if ((m = pathname.match(/^\/api\/runs\/([^/]+)\/citations$/))) {
    void m;
    return RUN_CITATIONS;
  }
  if ((m = pathname.match(/^\/api\/runs\/([^/]+)\/provenance$/))) {
    void m;
    return RUN_PROVENANCE;
  }
  if ((m = pathname.match(/^\/api\/runs\/([^/]+)\/responses$/))) {
    void m;
    return RUN_RESPONSES;
  }
  if ((m = pathname.match(/^\/api\/runs\/([^/]+)$/))) {
    return runDetailFor(decodeURIComponent(m[1]));
  }
  if ((m = pathname.match(/^\/v1\/plugins\/([^/]+)$/))) {
    const slug = decodeURIComponent(m[1]);
    const plugin = pluginFor(slug);
    // Fetcher treats a thrown error as "fall back to mock"; a 200 with the
    // plugin (or an empty object) keeps SSR clean.
    return plugin ?? {};
  }

  return undefined;
}

/** Match POST/PATCH routes. Returns [status, body] or undefined. */
function routeMutation(method, pathname, body) {
  let m;
  if (pathname === "/test/seed") return [200, { ok: true, seeded: true }];
  if (pathname === "/v1/projects") {
    return [201, createProject(body)];
  }
  if ((m = pathname.match(/^\/v1\/projects\/([^/]+)\/archive$/))) {
    archiveProject(decodeURIComponent(m[1]));
    return [200, { archived: true }];
  }
  if (pathname === "/v1/setup/clickhouse/connect") {
    return [200, { ok: true, state: "connected", message: "Connected.", endpoint: "https://api.tinybird.co" }];
  }
  if (pathname === "/v1/setup/clickhouse/install") {
    return [
      202,
      { install_id: "01HZMOCK", stream: "/v1/setup/clickhouse/install-stream?id=01HZMOCK" },
    ];
  }
  if (pathname === "/v1/setup/clickhouse/resume") {
    return [202, { triggered: true, message: "ETL resume re-armed." }];
  }
  if (pathname === "/v1/setup/webhook/test") {
    return [200, { status_code: 200, signature_valid: true, latency_ms: 42, error: null }];
  }
  if ((m = pathname.match(/^\/v1\/setup\/api-keys\/([^/]+)\/revoke$/))) {
    void m;
    return [204, ""];
  }
  if ((m = pathname.match(/^\/v1\/recommendations\/([^/]+)\/state$/))) {
    const rec = recDetailFor(decodeURIComponent(m[1]));
    return [200, { recommendation: rec, warnings: [] }];
  }
  if ((m = pathname.match(/^\/v1\/alert-rules\/([^/]+)$/))) {
    const name = decodeURIComponent(m[1]);
    return [200, { name, on: "", target: "*", channels: [], status: "muted", fires: 0 }];
  }
  if ((m = pathname.match(/^\/v1\/plugins\/([^/]+)\/install$/))) {
    const slug = decodeURIComponent(m[1]);
    return [
      200,
      {
        ok: true,
        signature_status: "signed",
        audit_event_id: `evt_${slug.replace(/\W+/g, "_")}`,
        message: "Plugin installed.",
      },
    ];
  }
  void method;
  return undefined;
}

const server = http.createServer((req, res) => {
  let pathname = "/";
  let searchParams = new URLSearchParams();
  try {
    const parsed = new URL(req.url, `http://localhost:${PORT}`);
    pathname = parsed.pathname;
    searchParams = parsed.searchParams;
  } catch {
    pathname = req.url ?? "/";
  }

  const method = (req.method ?? "GET").toUpperCase();

  // Read the canonical X-Anseo-Project header, with the legacy X-OpenGEO-Project
  // accepted as a fallback (mirrors the backend's back-compat acceptance).
  const projectHeader =
    req.headers["x-anseo-project"] !== undefined
      ? String(req.headers["x-anseo-project"])
      : req.headers["x-opengeo-project"] !== undefined
        ? String(req.headers["x-opengeo-project"])
        : undefined;

  // Buffer the request body so mutations (create project) can read it.
  const chunks = [];
  req.on("data", (c) => chunks.push(c));
  req.on("end", () => {
    const reqBody = Buffer.concat(chunks).toString("utf8");
    if (method === "GET" || method === "HEAD") {
      const body = routeGet(pathname, searchParams, projectHeader);
      if (body !== undefined) {
        return send(res, 200, body);
      }
      // SSE install stream stub (used only client-side; harmless via SSR).
      if (pathname === "/v1/setup/clickhouse/install-stream") {
        res.writeHead(200, { "Content-Type": "text/event-stream" });
        res.end(
          'event: install\ndata: {"step":"complete","progress":1,"log_line":"[mock] complete","at":"2026-05-31T09:00:00Z"}\n\n',
        );
        return;
      }
      // Unmatched read path: empty object, never 500.
      return send(res, 200, {});
    }

    if (method === "POST" || method === "PATCH" || method === "PUT" || method === "DELETE") {
      const result = routeMutation(method, pathname, reqBody);
      if (result) {
        const [status, body] = result;
        return send(res, status, body);
      }
      return send(res, 200, { ok: true });
    }

    return send(res, 200, {});
  });
});

// Bind IPv4 loopback explicitly. With a bare `listen(PORT)` the server binds
// the unspecified address, which on dual-stack CI runners (GitHub ubuntu has
// `::1 localhost` in /etc/hosts) leaves the dev server's Node `fetch` (undici)
// resolving OGEO_API_BASE_URL=`localhost` to `::1` first and failing to reach
// an IPv4-only listener — Playwright's own healthcheck still passes, so SSR
// fetches silently return empty and only the no-fallback pages (mcp/setup)
// break. Pinning both the bind and the URL to 127.0.0.1 removes the ambiguity.
server.listen(PORT, "127.0.0.1", () => {
  console.log(`[mock-api] listening on http://127.0.0.1:${PORT}`);
});
