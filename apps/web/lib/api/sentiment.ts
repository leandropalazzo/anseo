// Epic 30 Story 3 — sentiment surface (live).
// Wires to GET /api/visibility/sentiment (same root-gated lane as the rest of
// the visibility client). Backend: anseo_analytics::sentiment::sentiment_points.

import { getJson } from "./_client";

export interface SentimentPoint {
  prompt: string;
  provider: string;
  entity: string;
  /** YYYY-MM-DD */
  day: string;
  positive: number;
  neutral: number;
  negative: number;
  total: number;
  positive_share: number;
  neutral_share: number;
  negative_share: number;
  /** Mean sentiment score in [0, 100]. */
  average_score: number;
}

export interface SentimentResponse {
  window_days: number;
  points: SentimentPoint[];
}

export async function fetchSentiment(days = 30): Promise<SentimentResponse> {
  const qs = new URLSearchParams({ days: String(days) });
  return getJson(`/api/visibility/sentiment?${qs.toString()}`);
}
