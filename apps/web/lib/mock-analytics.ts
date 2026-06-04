/**
 * Analytics-specific mock data for UX-D (Visibility / Citations / Competitors).
 *
 * Built on top of `lib/mock.ts` (UX-C). Imports `shareOfVoice`, `genTrend`,
 * `CITATIONS`, `COMPETITORS_BASE`, `PROVIDERS`, `BRAND` from there; only
 * adds the leaderboard / movers / win-loss / citation-graph-fallback shapes
 * the analytics screens need.
 *
 * Source: _bmad-output/planning-artifacts/ux-redesign-2026-05-29/project/src/screens-analytics.jsx
 */

import type { Scenario } from "@/lib/mock";
import type { CitationGraph } from "@/lib/api";
import type { ProviderId } from "@/lib/provider-colors";

// ─── Delta leaderboard (Visibility) ──────────────────────────────────────────

export type DeltaDirection = "up" | "down" | "flat";

export interface DeltaLeaderboardRow {
  name: string;
  provider: ProviderId;
  delta: number;
  was: number;
  now: number;
  dir: DeltaDirection;
}

export function deltaLeaderboard(scenario: Scenario): DeltaLeaderboardRow[] {
  if (scenario === "drop") {
    return [
      { name: "vector-db", provider: "openai", delta: +4.2, was: 2.4, now: 6.6, dir: "down" },
      { name: "vector-db", provider: "anthropic", delta: +1.1, was: 3.1, now: 4.2, dir: "down" },
      { name: "observability", provider: "openai", delta: -0.8, was: 4.0, now: 3.2, dir: "up" },
      { name: "ai-search-eval", provider: "perplexity", delta: -1.5, was: 5.2, now: 3.7, dir: "up" },
      { name: "edge-runtime", provider: "gemini", delta: +0.2, was: 4.1, now: 4.3, dir: "flat" },
    ];
  }
  if (scenario === "new-competitor") {
    return [
      { name: "vector-db", provider: "gemini", delta: +0.9, was: 2.1, now: 3.0, dir: "down" },
      { name: "vector-db", provider: "openai", delta: +0.4, was: 2.4, now: 2.8, dir: "flat" },
      { name: "observability", provider: "anthropic", delta: -0.7, was: 4.0, now: 3.3, dir: "up" },
      { name: "rust-orm", provider: "openai", delta: -1.2, was: 6.1, now: 4.9, dir: "up" },
    ];
  }
  return [
    { name: "vector-db", provider: "openai", delta: -0.3, was: 2.4, now: 2.1, dir: "up" },
    { name: "observability", provider: "anthropic", delta: -1.0, was: 3.1, now: 2.1, dir: "up" },
    { name: "auth-saas", provider: "openai", delta: -0.2, was: 2.9, now: 2.7, dir: "up" },
    { name: "edge-runtime", provider: "gemini", delta: +0.4, was: 3.7, now: 4.1, dir: "down" },
  ];
}

// ─── Movers (Competitors) ────────────────────────────────────────────────────

export interface MoverRow {
  name: string;
  /** Delta in percentage points over the window. */
  deltaPp: number;
  /** First-seen marker ("10d ago", "—"). */
  firstSeen: string;
  /** True for new entrants (rendered with NEW pill). */
  isNew?: boolean;
}

export function movers(scenario: Scenario): MoverRow[] {
  if (scenario === "new-competitor") {
    return [
      { name: "turbopuffer", deltaPp: +18, firstSeen: "10d ago", isNew: true },
      { name: "qdrant", deltaPp: +3, firstSeen: "—" },
      { name: "lancedb", deltaPp: +2, firstSeen: "—" },
    ];
  }
  if (scenario === "drop") {
    return [
      { name: "pinecone", deltaPp: -7, firstSeen: "—" },
      { name: "qdrant", deltaPp: +4, firstSeen: "—" },
      { name: "weaviate", deltaPp: +2, firstSeen: "—" },
    ];
  }
  return [
    { name: "pinecone", deltaPp: +1, firstSeen: "—" },
    { name: "qdrant", deltaPp: 0, firstSeen: "—" },
    { name: "milvus", deltaPp: -1, firstSeen: "—" },
  ];
}

// ─── Win/Loss table (Competitors) ────────────────────────────────────────────

export interface WinLossRow {
  competitor: string;
  /** True when the competitor is ahead of us on this provider. */
  ahead: Readonly<Record<ProviderId, boolean>>;
  whereTheyWin: string;
}

const WIN_LOSS_COMPETITORS: readonly string[] = [
  "qdrant",
  "weaviate",
  "milvus",
  "chroma",
  "lancedb",
];

const WIN_LOSS_BLURBS: Readonly<Record<string, string>> = {
  qdrant: "OSS, hybrid search benchmarks",
  weaviate: "GraphQL DX, modular vectorizers",
  milvus: "scale to billions, on-prem",
  chroma: "prototype DX, lightweight",
  lancedb: "embedded use cases, S3-backed",
};

const ALL_PROVIDERS: readonly ProviderId[] = [
  "openai",
  "anthropic",
  "gemini",
  "perplexity",
];

export function winLoss(): WinLossRow[] {
  // Deterministic pattern matched to the prototype's `(i + p.length) % 3 === 0`
  // rule so screenshots line up. Real impl will read from analytics later.
  return WIN_LOSS_COMPETITORS.map((c, i) => {
    const ahead = {} as Record<ProviderId, boolean>;
    ALL_PROVIDERS.forEach((p) => {
      ahead[p] = (i + p.length) % 3 === 0;
    });
    return {
      competitor: c,
      ahead,
      whereTheyWin: WIN_LOSS_BLURBS[c] ?? "",
    };
  });
}

// ─── Citation graph fallback (matches /v1/analytics/citation-graph shape) ────

/**
 * Hand-built fallback graph used when the backend is unreachable. Shape is
 * IDENTICAL to `CitationGraph` from `lib/api.ts` (Story 14.2) so the
 * force-directed renderer never has to branch.
 */
export function mockCitationGraph(): CitationGraph {
  const providers: ProviderId[] = ["openai", "anthropic", "gemini", "perplexity"];
  const domains = [
    "news.ycombinator.com",
    "github.com",
    "reddit.com",
    "wikipedia.org",
    "qdrant.tech",
    "youtube.com",
    "docs.pinecone.io",
    "weaviate.io",
  ];

  const nodes = [
    ...providers.map((p) => ({ id: p, kind: "provider" as const, label: p })),
    ...domains.map((d) => ({ id: d, kind: "domain" as const, label: d })),
  ];

  // Deterministic weights so the layout is stable across runs.
  const edges = providers.flatMap((p, pi) =>
    domains.map((d, di) => ({
      source: p,
      target: d,
      weight: Math.max(1, 5 + ((pi * 7 + di * 3) % 9) - ((di + pi) % 3)),
    })),
  );

  return { nodes, edges };
}
