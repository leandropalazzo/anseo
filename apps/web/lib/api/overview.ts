// Overview feature.
//
// The Overview surface composes fetchers that live in other feature modules
// (runs, citations, competitors/brands). This module owns the overview-specific
// live fetchers: the prompt run-summary KPIs and the anomaly feed.

import { getJson } from "./_client";
import type { AnomalyItem } from "./anomalies";
import { fetchVisibilityTrend, type VisibilityPoint } from "./visibility";

/** One row of `GET /prompts/run-summary` — per-prompt rollups since `since`. */
export interface RunSummaryItem {
  prompt: string;
  run_count: number;
  last_run_at?: string;
  /** Fraction in [0,1] of runs that succeeded. */
  success_rate?: number;
  avg_latency_ms?: number;
  providers: string[];
}

/** Response shape of `GET /prompts/run-summary` (wire: RunSummaryResponse). */
export interface RunSummaryResponse {
  items: RunSummaryItem[];
  since: string;
}

/**
 * Fetch per-prompt run rollups. `since` is an RFC3339 timestamp; when omitted
 * the backend applies its default window. Overview KPI tiles aggregate the
 * returned rows (run counts, success rate, latency, distinct providers).
 */
export async function fetchRunSummary(
  since?: string,
): Promise<RunSummaryResponse> {
  const suffix = since ? `?since=${encodeURIComponent(since)}` : "";
  return getJson<RunSummaryResponse>(`/v1/prompts/run-summary${suffix}`);
}

/** One row of `GET /prompts/tag-summary` — per-tag rollup since `since`. */
export interface TagSummaryItem {
  tag: string;
  prompt_count: number;
  run_count: number;
  /** Fraction in [0,1] of runs that succeeded; null when run_count == 0. */
  success_rate?: number;
  providers: string[];
}

/** Response shape of `GET /prompts/tag-summary` (wire: TagSummaryResponse). */
export interface TagSummaryResponse {
  items: TagSummaryItem[];
  since: string;
}

/** Fetch per-tag run rollups for the Overview "by tag" summary. */
export async function fetchTagSummary(
  since?: string,
): Promise<TagSummaryResponse> {
  const suffix = since ? `?since=${encodeURIComponent(since)}` : "";
  return getJson<TagSummaryResponse>(`/v1/prompts/tag-summary${suffix}`);
}

/** Response shape of `GET /anomalies` (wire: AnomaliesResponse). */
export interface AnomaliesResponse {
  items: AnomalyItem[];
  trace_id: string;
}

/**
 * Live anomaly feed for the Overview timeline. `window` is a rolling window
 * string (`7d` etc.). Returns wire-stable `AnomalyItem`s; the caller passes
 * `items` straight into `<AnomalyTimeline items={...} />`.
 *
 * Named `fetchAnomalyFeed` (not `fetchAnomalies`) so it can be barrel-exported
 * alongside the mock `fetchAnomalies` in `./anomalies` without a name clash;
 * story 30-3 reconciles the two when it lands the live anomalies module.
 */
export async function fetchAnomalyFeed(
  window: string = "7d",
): Promise<AnomaliesResponse> {
  return getJson<AnomaliesResponse>(
    `/anomalies?window=${encodeURIComponent(window)}`,
  );
}

/**
 * Real per-bucket trend series for the Overview KPI sparklines.
 *
 * Drives sparklines from genuine history rather than synthesizing one around an
 * aggregate mean. Fetches `GET /api/visibility/trend` for a representative
 * prompt and collapses the per-(bucket, provider) points into one value per
 * bucket (mean across providers), ordered oldest→newest:
 *
 * - `ranks`    — mean `avg_rank` over providers reporting a rank in the bucket.
 * - `presence` — mean `presence_rate` over providers in the bucket.
 *
 * Returns empty arrays when the prompt has no buckets (or the fetch fails),
 * so the caller renders the KPI number with no sparkline instead of a fake one.
 */
export interface OverviewTrendSeries {
  ranks: number[];
  presence: number[];
}

export async function fetchOverviewTrendSeries(
  prompt: string,
  days = 7,
): Promise<OverviewTrendSeries> {
  let points: VisibilityPoint[] = [];
  try {
    const r = await fetchVisibilityTrend(prompt, days);
    points = Array.isArray(r.points) ? r.points : [];
  } catch {
    return { ranks: [], presence: [] };
  }
  return bucketSeries(points);
}

/** One hourly bucket of project-wide KPIs (wire: KpiTrendPoint). */
export interface KpiTrendPoint {
  bucket_start: string;
  run_count: number;
  success_rate: number;
  avg_latency_ms: number | null;
}

/**
 * Hourly project-wide KPI trend for the Overview tile sparklines. Returns the
 * raw per-bucket points oldest→newest; callers map out the series they need
 * (run_count / success_rate / avg_latency_ms). Empty on fetch failure so the
 * tile renders its number with no sparkline.
 */
export async function fetchKpiTrend(hours = 168): Promise<KpiTrendPoint[]> {
  try {
    const r = await getJson<{ points: KpiTrendPoint[] }>(
      `/v1/prompts/kpi-trend?hours=${hours}`,
    );
    return Array.isArray(r.points) ? r.points : [];
  } catch {
    return [];
  }
}

/** Collapse per-(bucket, provider) points into one value per bucket. */
function bucketSeries(points: VisibilityPoint[]): OverviewTrendSeries {
  const byBucket = new Map<
    string,
    { rankSum: number; rankCount: number; presSum: number; presCount: number }
  >();
  for (const p of points) {
    const acc =
      byBucket.get(p.bucket_start) ??
      { rankSum: 0, rankCount: 0, presSum: 0, presCount: 0 };
    if (p.avg_rank !== null && Number.isFinite(p.avg_rank)) {
      acc.rankSum += p.avg_rank;
      acc.rankCount += 1;
    }
    if (Number.isFinite(p.presence_rate)) {
      acc.presSum += p.presence_rate;
      acc.presCount += 1;
    }
    byBucket.set(p.bucket_start, acc);
  }
  const buckets = [...byBucket.keys()].sort();
  const ranks: number[] = [];
  const presence: number[] = [];
  for (const b of buckets) {
    const acc = byBucket.get(b)!;
    if (acc.rankCount > 0) ranks.push(acc.rankSum / acc.rankCount);
    if (acc.presCount > 0) presence.push(acc.presSum / acc.presCount);
  }
  return { ranks, presence };
}
