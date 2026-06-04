// Visibility feature: trend + schedule fetchers.

import { getJson } from "./_client";

export interface VisibilityPoint {
  bucket_start: string;
  provider: string;
  avg_rank: number | null;
  presence_rate: number;
}

export async function fetchVisibilityTrend(
  prompt: string,
  days: number,
): Promise<{ points: VisibilityPoint[] }> {
  const qs = new URLSearchParams({ prompt, days: String(days) });
  return getJson(`/api/visibility/trend?${qs.toString()}`);
}

/** One (prompt × provider) cell of the overall visibility matrix. */
export interface VisibilityMatrixCell {
  prompt_name: string;
  provider: string;
  run_count: number;
  mention_count: number;
  presence_rate: number;
  avg_rank: number | null;
}

export interface VisibilityOverall {
  brand: string;
  window_days: number;
  matrix: VisibilityMatrixCell[];
  trend: VisibilityPoint[];
}

/** Overall visibility across ALL prompts: a prompt×provider matrix plus an
 *  all-prompts day-by-day trend. Powers the "Overall" tab. */
export async function fetchVisibilityOverall(
  days: number,
): Promise<VisibilityOverall> {
  const qs = new URLSearchParams({ days: String(days) });
  return getJson(`/api/visibility/overall?${qs.toString()}`);
}

export interface ScheduleSummary {
  id: string;
  name: string;
  cron: string;
  prompts: string[];
  providers: string[];
  debounce_minutes: number;
  projected_monthly_usd: number | null;
  projection_acknowledged_at: string | null;
  paused: boolean;
  created_at: string;
  last_tick_at: string | null;
  last_tick_status: string | null;
}

export async function fetchSchedules(): Promise<{ schedules: ScheduleSummary[] }> {
  return getJson(`/v1/schedules`);
}
