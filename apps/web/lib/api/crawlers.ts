// Epic 31 / Epic 33 — AI-crawler observability + crawl-to-refer ratio (live).
// Wires to GET /v1/crawlers/metrics and GET /v1/crawlers/ratio.

import { getJson } from "./_client";

export interface CrawlerBotMetric {
  bot_id: string;
  hits: number;
  verified_hits: number;
  error_hits: number;
}

export interface CrawlerPathMetric {
  path: string;
  hits: number;
  error_hits: number;
}

export interface CrawlerTrendBucket {
  /** YYYY-MM-DD */
  day: string;
  hits: number;
}

export interface CrawlerMetrics {
  window_start: string;
  window_end: string;
  include_unverified: boolean;
  bots: CrawlerBotMetric[];
  top_paths: CrawlerPathMetric[];
  error_paths: CrawlerPathMetric[];
  trend: CrawlerTrendBucket[];
}

export async function fetchCrawlerMetrics(
  days = 30,
  includeUnverified = false,
): Promise<CrawlerMetrics> {
  const qs = new URLSearchParams({
    days: String(days),
    include_unverified: String(includeUnverified),
  });
  return getJson(`/v1/crawlers/metrics?${qs.toString()}`);
}

/** Degraded-honestly state: `crawls_only` until referral attribution lands. */
export type CrawlReferState = "ok" | "crawls_only";

export interface CrawlReferBot {
  bot_id: string;
  verified_crawl_hits: number;
  attributed_referrals: number;
  /** null while in `crawls_only` state (no referral attribution yet). */
  ratio: number | null;
}

export interface CrawlReferReport {
  window_start: string;
  window_end: string;
  state: CrawlReferState;
  bots: CrawlReferBot[];
}

export async function fetchCrawlReferRatio(days = 30): Promise<CrawlReferReport> {
  const qs = new URLSearchParams({ days: String(days) });
  return getJson(`/v1/crawlers/ratio?${qs.toString()}`);
}

export interface IngestResult {
  /** Lines that parsed into a recognized crawler hit. */
  parsed: number;
  /** Normalized events newly written (idempotent on source + raw_event_id). */
  ingested: number;
  /** Lines that did not parse (malformed or non-crawler user-agents). */
  skipped: number;
}

export interface IngestRequest {
  lines: string[];
  format?: "common" | "combined";
  privacy_mode?: "hashed" | "truncated" | "raw";
}

/** Paste-logs ingest. Client-triggered → routes through the same-origin Next
 *  handler so the operator API key is attached server-side. */
export async function ingestCrawlerLogs(req: IngestRequest): Promise<IngestResult> {
  const r = await fetch("/api/crawlers/ingest", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!r.ok) throw new Error(`crawler ingest failed (${r.status})`);
  return (await r.json()) as IngestResult;
}
