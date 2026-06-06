// Story 47.4 — operator site-analytics dashboard fetchers.
//
// Reads the two operator-scoped endpoints that aggregate the public-site event
// rollups (privacy-safe by construction — never raw per-visitor rows):
//   * GET /v1/analytics/site-overview?period=7d|30d
//   * GET /v1/analytics/funnels?period=7d|30d
//
// Both ride the API operator surface (require_api_key, no X-Anseo-Project
// guard). Reads run server-side via `getJson`, which attaches the operator API
// key from server env (the key is never exposed to the browser). Each fetcher
// degrades to an empty payload on error so the page renders its zero-state
// instead of crashing (AC-5).

import { getJson } from "./_client";

export type AnalyticsPeriod = "7d" | "30d";

/** Normalize an arbitrary query value to a supported period (defaults to 7d). */
export function normalizePeriod(value: string | undefined): AnalyticsPeriod {
  return value === "30d" ? "30d" : "7d";
}

// ─── Site overview ───────────────────────────────────────────────────────────

export interface DayCount {
  date: string;
  count: number;
}

export interface TopPage {
  path: string;
  views: number;
}

export interface TopReferrer {
  domain: string;
  visits: number;
}

export interface SiteOverview {
  period_days: number;
  sessions_per_day: DayCount[];
  top_pages: TopPage[];
  top_referrers: TopReferrer[];
}

const EMPTY_OVERVIEW: SiteOverview = {
  period_days: 7,
  sessions_per_day: [],
  top_pages: [],
  top_referrers: [],
};

export async function fetchSiteOverview(
  period: AnalyticsPeriod,
): Promise<SiteOverview> {
  try {
    return await getJson<SiteOverview>(
      `/v1/analytics/site-overview?period=${period}`,
    );
  } catch {
    return { ...EMPTY_OVERVIEW };
  }
}

// ─── Funnels ─────────────────────────────────────────────────────────────────

export interface FunnelStep {
  label: string;
  count: number;
  /** Drop-off from the previous step (0..100). `null` for the first step and
   *  for the "tracking deployed mid-funnel" anomaly (later step grew). */
  drop_off_pct: number | null;
}

export interface VerifyMethod {
  method: string;
  start: number;
  complete: number;
  fail: number;
  /** complete / start as 0..100; `null` when start === 0. */
  success_rate_pct: number | null;
}

export interface Funnels {
  period_days: number;
  contribute: FunnelStep[];
  verify: VerifyMethod[];
  badge_embeds_per_day: DayCount[];
}

const EMPTY_FUNNELS: Funnels = {
  period_days: 7,
  contribute: [],
  verify: [],
  badge_embeds_per_day: [],
};

export async function fetchFunnels(period: AnalyticsPeriod): Promise<Funnels> {
  try {
    return await getJson<Funnels>(`/v1/analytics/funnels?period=${period}`);
  } catch {
    return { ...EMPTY_FUNNELS };
  }
}

/** True when every panel's data is empty — drives the page-level zero-state. */
export function isAnalyticsEmpty(
  overview: SiteOverview,
  funnels: Funnels,
): boolean {
  return (
    overview.sessions_per_day.length === 0 &&
    overview.top_pages.length === 0 &&
    overview.top_referrers.length === 0 &&
    funnels.contribute.every((s) => s.count === 0) &&
    funnels.verify.length === 0 &&
    funnels.badge_embeds_per_day.length === 0
  );
}
