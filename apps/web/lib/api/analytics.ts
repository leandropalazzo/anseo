// Story 14.2 / 14.3 / 14.4 analytics endpoints (live).

import { getJson } from "./_client";

export type CitationGraphNodeKind = "provider" | "domain";

export interface CitationGraphNode {
  id: string;
  kind: CitationGraphNodeKind;
  label: string;
}

export interface CitationGraphEdge {
  source: string;
  target: string;
  weight: number;
}

export interface CitationGraph {
  nodes: CitationGraphNode[];
  edges: CitationGraphEdge[];
}

export async function fetchCitationGraph(
  days = 30,
): Promise<CitationGraph> {
  return getJson(`/v1/analytics/citation-graph?days=${days}`);
}

export interface HeatmapCell {
  /** YYYY-MM-DD */
  date: string;
  provider: string;
  runs: number;
  presence_rate: number;
  avg_rank: number | null;
}

export interface Heatmap {
  cells: HeatmapCell[];
}

export async function fetchHeatmap(
  brand: string,
  days = 30,
): Promise<Heatmap> {
  const qs = new URLSearchParams({ brand, days: String(days) });
  return getJson(`/v1/analytics/heatmap?${qs.toString()}`);
}

export interface Volatility {
  /** Coefficient-of-variation clamped to [0, 1]; null when window has zero non-null samples. */
  value: number | null;
  presence_ratio: number;
  samples: number;
}

export async function fetchVolatility(args: {
  prompt: string;
  provider: string;
  brand: string;
  window?: number;
}): Promise<Volatility> {
  const qs = new URLSearchParams({
    prompt: args.prompt,
    provider: args.provider,
    brand: args.brand,
  });
  if (args.window !== undefined) qs.set("window", String(args.window));
  return getJson(`/v1/analytics/volatility?${qs.toString()}`);
}
