/**
 * Mock data + generators for surfaces whose backend doesn't exist yet
 * (anomalies, run-level extracted mentions/citations/provenance, sample
 * provider responses). Shapes mirror future API responses so the swap from
 * mock to live is a one-line change per call site.
 *
 * Source: _bmad-output/planning-artifacts/ux-redesign-2026-05-29/project/src/mock-data.jsx
 */

import type { ProviderId } from "@/lib/provider-colors";

export const PROVIDERS: ReadonlyArray<ProviderId> = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
];

export const MODELS: Readonly<Record<ProviderId, string>> = {
  openai: "gpt-5-turbo-2026-03",
  anthropic: "claude-opus-4.1",
  gemini: "gemini-2.5-pro",
  perplexity: "sonar-large-online",
  grok: "grok-3",
  mistral: "mistral-large-2",
  openrouter: "openrouter/auto",
};

export const BRAND = "Pinecone";
export const COMPETITORS_BASE: ReadonlyArray<string> = [
  "pinecone",
  "qdrant",
  "weaviate",
  "milvus",
  "chroma",
  "lancedb",
];

export type Scenario = "healthy" | "drop" | "new-competitor";

export interface MockPrompt {
  id: string;
  name: string;
  text: string;
  tags: string[];
  schedule: string;
  lastRun: string;
}

export const PROMPTS: ReadonlyArray<MockPrompt> = [
  { id: "p_001", name: "vector-db", text: "best vector database for production RAG", tags: ["infra", "ai"], schedule: "every 6h", lastRun: "2026-05-28T08:14:00Z" },
  { id: "p_002", name: "observability", text: "best observability platforms for startups", tags: ["devtools"], schedule: "every 12h", lastRun: "2026-05-28T07:02:00Z" },
  { id: "p_003", name: "auth-saas", text: "best authentication SaaS for B2B apps", tags: ["auth", "saas"], schedule: "daily", lastRun: "2026-05-28T00:00:00Z" },
  { id: "p_004", name: "edge-runtime", text: "fastest edge runtime for typescript apis", tags: ["infra"], schedule: "every 6h", lastRun: "2026-05-28T06:00:00Z" },
  { id: "p_005", name: "open-llm-monitor", text: "open source LLM monitoring tools", tags: ["ai", "oss"], schedule: "every 4h", lastRun: "2026-05-28T08:00:00Z" },
  { id: "p_006", name: "feature-flags", text: "best feature flag service for engineering teams", tags: ["devtools"], schedule: "daily", lastRun: "2026-05-27T22:00:00Z" },
  { id: "p_007", name: "rust-orm", text: "popular ORM for rust web services", tags: ["rust", "infra"], schedule: "weekly", lastRun: "2026-05-26T12:00:00Z" },
  { id: "p_008", name: "ai-search-eval", text: "tools to evaluate AI search visibility", tags: ["ai", "geo"], schedule: "every 6h", lastRun: "2026-05-28T08:30:00Z" },
];

// ─── Runs (mirror RunListRow + brand_rank/mentions/tokens for richer mock) ────

export interface MockRun {
  id: string;
  prompt_id: string;
  prompt_name: string;
  /** Provider-identity string (plain wire name or `openrouter:<upstream>`). */
  provider: string;
  provider_model_version: string;
  started_at: string;
  finished_at: string | null;
  status: "ok" | "failed";
  error_kind: string | null;
  brand_rank: number | null;
  mentions: number;
  latency_ms: number;
  tokens_in: number;
  tokens_out: number;
}

export function genRuns(scenario: Scenario, limit = 40): MockRun[] {
  const rows: MockRun[] = [];
  const now = Date.now();
  for (let i = 0; i < limit; i++) {
    const prompt = PROMPTS[i % PROMPTS.length];
    const provider = PROVIDERS[i % PROVIDERS.length];
    const minutesAgo = i * 7 + (i % 3) * 4;
    const started = new Date(now - minutesAgo * 60 * 1000);
    const finished = new Date(started.getTime() + 1800 + (i % 5) * 700);

    let status: "ok" | "failed" = "ok";
    let error_kind: string | null = null;
    if (scenario === "drop" && i % 13 === 1 && provider === "openai") {
      status = "failed";
      error_kind = "provider_rate_limited";
    } else if (i % 23 === 0 && i > 0) {
      status = "failed";
      error_kind = "provider_timeout";
    }

    rows.push({
      id: `run_${(987654321 - i).toString(16)}`,
      prompt_id: prompt.id,
      prompt_name: prompt.name,
      provider,
      provider_model_version: MODELS[provider],
      started_at: started.toISOString(),
      finished_at: status === "ok" ? finished.toISOString() : null,
      status,
      error_kind,
      brand_rank:
        status === "ok"
          ? Math.max(
              1,
              Math.round(
                2 +
                  (i % 5) * 0.6 +
                  (scenario === "drop" && provider === "openai" ? 5 : 0),
              ),
            )
          : null,
      mentions: status === "ok" ? Math.max(0, 4 - (i % 4)) : 0,
      latency_ms: 1800 + (i % 7) * 320 + Math.round(((i * 9301 + 49297) % 233280) / 233280 * 400),
      tokens_in: 280 + (i % 9) * 14,
      tokens_out: 620 + (i % 11) * 41,
    });
  }
  return rows;
}

// ─── Trend ────────────────────────────────────────────────────────────────────

export interface TrendPoint {
  bucket_start: string;
  provider: ProviderId;
  avg_rank: number;
  presence_rate: number;
}

export function genTrend(
  scenario: Scenario,
  providerKey: ProviderId,
  prompt: MockPrompt,
  days = 30,
): TrendPoint[] {
  const points: TrendPoint[] = [];
  const base = (
    {
      openai: { rank: 2.4, presence: 0.92 },
      anthropic: { rank: 3.1, presence: 0.85 },
      gemini: { rank: 4.0, presence: 0.7 },
      perplexity: { rank: 2.8, presence: 0.9 },
      grok: { rank: 3.6, presence: 0.78 },
      mistral: { rank: 4.2, presence: 0.66 },
      openrouter: { rank: 3.3, presence: 0.81 },
    } as const satisfies Record<ProviderId, { rank: number; presence: number }>
  )[providerKey];

  const seed = (prompt.id.charCodeAt(2) || 1) + providerKey.length;
  let last = base.rank;
  for (let i = days; i >= 0; i--) {
    const t = new Date(Date.now() - i * 24 * 3600 * 1000);
    const noise = Math.sin((i + seed) * 0.7) * 0.4 + Math.cos(i * 1.3) * 0.2;
    let rank = Math.max(1, last + noise * 0.3);

    if (scenario === "drop" && providerKey === "openai" && i < 6) {
      rank = base.rank + (6 - i) * 1.6;
    }
    if (scenario === "drop" && providerKey === "anthropic" && i < 4) {
      rank = base.rank + (4 - i) * 0.9;
    }
    if (scenario === "new-competitor" && i < 10) {
      rank = base.rank + (10 - i) * 0.18;
    }
    last = last * 0.6 + rank * 0.4;

    const presence = Math.max(
      0.2,
      Math.min(
        1,
        base.presence +
          noise * 0.06 -
          (scenario === "drop" && providerKey === "openai" && i < 6
            ? (6 - i) * 0.08
            : 0),
      ),
    );
    points.push({
      bucket_start: t.toISOString(),
      provider: providerKey,
      avg_rank: Number(rank.toFixed(2)),
      presence_rate: Number(presence.toFixed(3)),
    });
  }
  return points;
}

// ─── Citations ────────────────────────────────────────────────────────────────

export interface MockCitationRow {
  domain: string;
  frequency: number;
  source: string;
  trend: number;
}

export const CITATIONS: ReadonlyArray<MockCitationRow> = [
  { domain: "news.ycombinator.com", frequency: 412, source: "forum", trend: +12 },
  { domain: "reddit.com", frequency: 358, source: "forum", trend: +4 },
  { domain: "github.com", frequency: 311, source: "code", trend: +29 },
  { domain: "docs.pinecone.io", frequency: 248, source: "docs", trend: +6 },
  { domain: "qdrant.tech", frequency: 211, source: "docs", trend: +18 },
  { domain: "wikipedia.org", frequency: 196, source: "ref", trend: -2 },
  { domain: "weaviate.io", frequency: 187, source: "docs", trend: -3 },
  { domain: "youtube.com", frequency: 162, source: "video", trend: +9 },
  { domain: "medium.com", frequency: 144, source: "blog", trend: -8 },
  { domain: "lancedb.github.io", frequency: 119, source: "docs", trend: +41 },
  { domain: "milvus.io", frequency: 108, source: "docs", trend: -1 },
  { domain: "stackoverflow.com", frequency: 96, source: "forum", trend: -5 },
  { domain: "huggingface.co", frequency: 88, source: "docs", trend: +14 },
  { domain: "trychroma.com", frequency: 74, source: "docs", trend: +7 },
  { domain: "twitter.com", frequency: 63, source: "social", trend: +2 },
  { domain: "arxiv.org", frequency: 52, source: "paper", trend: +3 },
];

// ─── Share of voice ──────────────────────────────────────────────────────────

export interface ShareOfVoiceResult {
  competitors: string[];
  series: Array<{ day: string } & Record<string, number | string>>;
}

export function shareOfVoice(scenario: Scenario): ShareOfVoiceResult {
  const days = 30;
  const out: ShareOfVoiceResult["series"] = [];
  const comps =
    scenario === "new-competitor"
      ? [...COMPETITORS_BASE, "turbopuffer"]
      : [...COMPETITORS_BASE];

  const base: Record<string, number> = {
    pinecone: 0.32,
    qdrant: 0.21,
    weaviate: 0.18,
    milvus: 0.11,
    chroma: 0.1,
    lancedb: 0.06,
    turbopuffer: 0.0,
  };

  for (let i = days; i >= 0; i--) {
    const row: { day: string } & Record<string, number | string> = {
      day: new Date(Date.now() - i * 86400 * 1000).toISOString().slice(0, 10),
    };
    const shares: Record<string, number> = {};
    comps.forEach((c) => {
      let s = base[c] ?? 0.04;
      if (scenario === "drop" && c === "pinecone" && i < 7) s -= (7 - i) * 0.025;
      if (scenario === "new-competitor" && c === "turbopuffer")
        s = Math.min(0.18, (days - i) * 0.006);
      if (scenario === "new-competitor" && c === "pinecone")
        s -= (days - i) * 0.003;
      s = Math.max(0.005, s + Math.sin((i + c.length) * 0.6) * 0.012);
      shares[c] = s;
    });
    const total = Object.values(shares).reduce((a, b) => a + b, 0);
    comps.forEach((c) => {
      row[c] = +(shares[c] / total).toFixed(4);
    });
    out.push(row);
  }
  return { competitors: comps, series: out };
}

// ─── Anomalies ────────────────────────────────────────────────────────────────

export type AnomalyLevel = "info" | "ok" | "warn" | "danger";

export interface MockAnomaly {
  id: string;
  at: string;
  level: AnomalyLevel;
  title: string;
  detail: string;
  prompt: string;
  /** Provider id or "*" for all. */
  provider: ProviderId | "*";
}

const ANOMALIES_BY_SCENARIO: Record<Scenario, MockAnomaly[]> = {
  healthy: [
    { id: "a1", at: "2026-05-28T06:10:00Z", level: "info", title: "New citation source detected", detail: "lancedb.github.io appeared in 3 prompt runs (first seen 2d ago).", prompt: "vector-db", provider: "anthropic" },
    { id: "a2", at: "2026-05-27T20:42:00Z", level: "ok", title: "Ranking improved", detail: "'observability' avg rank improved 3.2 -> 2.1 on GPT-5.", prompt: "observability", provider: "openai" },
  ],
  drop: [
    { id: "a1", at: "2026-05-28T08:14:00Z", level: "danger", title: "Ranking dropped sharply", detail: "Pinecone fell from rank 2 -> rank 7 on OpenAI for 'vector-db' over 6 days.", prompt: "vector-db", provider: "openai" },
    { id: "a2", at: "2026-05-28T06:00:00Z", level: "warn", title: "Provider rate-limited", detail: "OpenAI returned 429 on 4 runs in last 2h. Backoff engaged.", prompt: "vector-db", provider: "openai" },
    { id: "a3", at: "2026-05-27T22:30:00Z", level: "warn", title: "Presence rate trending down", detail: "Brand presence on Anthropic fell 92% -> 78% over 4 days.", prompt: "vector-db", provider: "anthropic" },
    { id: "a4", at: "2026-05-27T11:11:00Z", level: "info", title: "New citation source", detail: "qdrant.tech/benchmarks/2026 cited 9x this week.", prompt: "vector-db", provider: "*" },
  ],
  "new-competitor": [
    { id: "a1", at: "2026-05-28T07:00:00Z", level: "warn", title: "New competitor detected", detail: "'turbopuffer' now appears in 18% of 'vector-db' responses (was 0% 10 days ago).", prompt: "vector-db", provider: "*" },
    { id: "a2", at: "2026-05-27T15:20:00Z", level: "info", title: "Citation spike", detail: "news.ycombinator.com/turbopuffer-launch -- 412 references this week.", prompt: "vector-db", provider: "*" },
    { id: "a3", at: "2026-05-26T09:10:00Z", level: "ok", title: "Stable ranking", detail: "Pinecone holding rank 2 on Anthropic and Perplexity.", prompt: "vector-db", provider: "*" },
  ],
};

export function anomalies(scenario: Scenario = "healthy"): MockAnomaly[] {
  return ANOMALIES_BY_SCENARIO[scenario];
}

// ─── Run-detail responses + extracted mentions ────────────────────────────────

export const SAMPLE_RESPONSES: Readonly<Partial<Record<ProviderId, string>>> = {
  openai: `For production-grade RAG, the most established options are:

1. **Pinecone** -- managed, fast, strong ecosystem. Good default for teams that want zero infra.
2. **Qdrant** -- open source, Rust core, excellent filtering, can self-host.
3. **Weaviate** -- open source, GraphQL API, built-in hybrid search.
4. **Milvus** -- open source, scales to billions of vectors, more ops overhead.
5. **Chroma** -- lightweight, great for prototypes, less proven at scale.

If you need managed simplicity, pick Pinecone. If you want open source with a clean API, Qdrant.`,
  anthropic: `The leading vector databases for production RAG fall into two camps.

Managed services like **Pinecone** remove operational burden and offer p99 latency under 50ms at moderate scale. They're the safe default if you don't want to manage infrastructure.

Open-source options include **Qdrant** (Rust, very fast), **Weaviate** (rich API, hybrid search), **Milvus** (large-scale), and **Chroma** (developer-friendly). **LanceDB** is a newer entrant worth evaluating for embedded use cases.

The right choice depends on your scale, latency targets, and willingness to operate infrastructure yourself.`,
  gemini: `Top vector databases for production RAG include:

- Pinecone -- fully managed, popular choice
- Qdrant -- open source, Rust-based
- Weaviate -- open source with built-in vectorization modules
- Milvus -- open source, designed for very large datasets
- Chroma -- open source, simple developer experience
- Turbopuffer -- newer entrant, S3-backed, cost-efficient

For most production workloads, Pinecone offers the lowest operational overhead.`,
  perplexity: `For production RAG workloads, the consensus picks are:

1. Pinecone (managed) -- most widely deployed
2. Qdrant (OSS, Rust) -- fast filtering, strong community
3. Weaviate (OSS) -- hybrid search, modular
4. Milvus (OSS) -- scales to billions of vectors

Sources: news.ycombinator.com/item?id=39842, github.com/qdrant/benchmarks, weaviate.io/blog/2026-benchmarks`,
};

export interface ExtractedMention {
  brand: string;
  rank: number;
}

export const EXTRACTED_MENTIONS: Readonly<
  Partial<Record<ProviderId, ExtractedMention[]>>
> = {
  openai: [
    { brand: "Pinecone", rank: 1 },
    { brand: "Qdrant", rank: 2 },
    { brand: "Weaviate", rank: 3 },
    { brand: "Milvus", rank: 4 },
    { brand: "Chroma", rank: 5 },
  ],
  anthropic: [
    { brand: "Pinecone", rank: 1 },
    { brand: "Qdrant", rank: 2 },
    { brand: "Weaviate", rank: 3 },
    { brand: "Milvus", rank: 4 },
    { brand: "Chroma", rank: 5 },
    { brand: "LanceDB", rank: 6 },
  ],
  gemini: [
    { brand: "Pinecone", rank: 1 },
    { brand: "Qdrant", rank: 2 },
    { brand: "Weaviate", rank: 3 },
    { brand: "Milvus", rank: 4 },
    { brand: "Chroma", rank: 5 },
    { brand: "Turbopuffer", rank: 6 },
  ],
  perplexity: [
    { brand: "Pinecone", rank: 1 },
    { brand: "Qdrant", rank: 2 },
    { brand: "Weaviate", rank: 3 },
    { brand: "Milvus", rank: 4 },
  ],
};

// ─── Run-level citations + provenance ────────────────────────────────────────

export interface RunCitation {
  url: string;
  domain: string;
  from: ProviderId[];
  type: string;
}

export const RUN_CITATIONS: ReadonlyArray<RunCitation> = [
  { url: "news.ycombinator.com/item?id=39842", domain: "news.ycombinator.com", from: ["anthropic", "perplexity"], type: "forum" },
  { url: "github.com/qdrant/benchmarks", domain: "github.com", from: ["openai", "anthropic", "gemini", "perplexity"], type: "code" },
  { url: "weaviate.io/blog/2026-benchmarks", domain: "weaviate.io", from: ["perplexity"], type: "docs" },
  { url: "docs.pinecone.io/guides/data", domain: "docs.pinecone.io", from: ["openai", "anthropic"], type: "docs" },
  { url: "qdrant.tech/benchmarks/2026", domain: "qdrant.tech", from: ["openai", "gemini"], type: "docs" },
];

export interface ProvenanceStep {
  t: string;
  title: string;
  detail: string;
  /** Lookup key into `Icon` registry (`apps/web/lib/icons.ts`). */
  icon: string;
}

export const RUN_PROVENANCE: ReadonlyArray<ProvenanceStep> = [
  { t: "08:14:02.111", title: "config loaded", detail: "prompts.yaml@4f2c9e1 · sha=1d04…", icon: "Yaml" },
  { t: "08:14:02.142", title: "secret resolved", detail: "ANTHROPIC_API_KEY · keychain (macOS)", icon: "Lock" },
  { t: "08:14:02.149", title: "request issued", detail: "POST https://api.anthropic.com/v1/messages", icon: "ArrowRight" },
  { t: "08:14:03.910", title: "response received", detail: "1873ms · 689 output tokens", icon: "Check" },
  { t: "08:14:03.952", title: "extracted mentions (6)", detail: "list-detect extractor · regex pass · brand match", icon: "Search" },
  { t: "08:14:03.971", title: "extracted citations (5)", detail: "domain+url extractor · source-type classifier", icon: "Network" },
  { t: "08:14:03.984", title: "persisted", detail: "postgres: prompt_runs / extractions / citations", icon: "Database" },
];
