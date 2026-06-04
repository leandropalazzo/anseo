// Citations feature: live citation summary + mock per-run citations.

import { getJson } from "./_client";
import { RUN_CITATIONS, type RunCitation } from "../mock";

export interface CitationSummaryRow {
  domain: string;
  frequency: number;
  source_type: string | null;
}

/** Composite citation-health score (0–100) with its sub-components, so the
 *  headline number stays explainable. Mirrors the API `CitationScore`. */
export interface CitationScore {
  score: number;
  total_citations: number;
  distinct_domains: number;
  quality_share: number;
  growth_rate: number | null;
  volume_component: number;
  diversity_component: number;
  quality_component: number;
}

export interface CitationSummary {
  domains: CitationSummaryRow[];
  citation_score: CitationScore;
  window_days: number;
}

export async function fetchCitationSummary(
  limit = 50,
): Promise<CitationSummary> {
  return getJson(`/api/citations/summary?limit=${limit}`);
}

/** One hourly point of a single domain's citation-frequency trend. */
export interface CitationTrendPoint {
  bucket_start: string;
  frequency: number;
}

/** Per-domain hourly citation-frequency series, keyed by domain. Powers the
 *  sparkline column in the citations table. */
export async function fetchCitationTrend(
  hours = 168,
  limit = 50,
): Promise<Record<string, CitationTrendPoint[]>> {
  const r = await getJson<{ trend: Record<string, CitationTrendPoint[]> }>(
    `/api/citations/trend?hours=${hours}&limit=${limit}`,
  );
  return r.trend && typeof r.trend === "object" && !Array.isArray(r.trend)
    ? r.trend
    : {};
}

/** Per-run citations list (domain + provenance per provider). Mock only. */
export async function fetchRunCitations(
  runId: string,
): Promise<{ citations: RunCitation[] }> {
  void runId;
  return { citations: [...RUN_CITATIONS] };
}
