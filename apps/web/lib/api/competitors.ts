// Competitors feature (Story 30-2).
//
// Live fetchers for the competitor surfaces. Backed by two already-live API
// endpoints:
//   GET /comparisons?brands=<b1,b2,...>&window=1d|7d|30d  → CompareBrandsOutput
//   GET /brands                                           → BrandsResponse
//
// The competitors page (a server component) calls these during render and
// derives share-of-voice / win-loss / head-to-head / movers from the returned
// `rows[].cells[]` + `brands.items`. See `app/competitors/page.tsx`.

import { getJson } from "./_client";

/** A single brand/competitor cell within a prompt×provider row. */
export interface ComparisonCell {
  /** The brand or competitor this cell measures. */
  subject: string;
  /** 1-based ranking when the subject was ranked by the provider, else absent. */
  ranking?: number;
  /** How many times the subject was mentioned in this prompt×provider run. */
  mention_count: number;
}

/** One prompt×provider row of the comparison matrix. */
export interface ComparisonRow {
  prompt_id: string;
  prompt_name: string;
  provider: string;
  cells: ComparisonCell[];
}

/** Response shape of `GET /comparisons` (wire: CompareBrandsOutput). */
export interface CompareBrandsOutput {
  window: string;
  /** The primary brand the comparison is centred on. */
  brand: string;
  /** The competitor brands compared against the primary brand. */
  competitors: string[];
  rows: ComparisonRow[];
  trace_id: string;
}

/** One brand entry from `GET /brands`. */
export interface BrandItem {
  name: string;
  is_primary: boolean;
  mention_count_7d: number;
  avg_rank_7d?: number | null;
  providers_with_data: string[];
}

/** Response shape of `GET /brands` (wire: BrandsResponse). */
export interface BrandsResponse {
  items: BrandItem[];
}

export type ComparisonWindow = "1d" | "7d" | "30d";

/**
 * Fetch the head-to-head comparison matrix for `brands` over `window`.
 * Brands are passed as a comma-separated `brands` query param; the first is
 * conventionally the primary brand.
 */
export async function fetchComparisons(
  brands: string[],
  window: ComparisonWindow = "7d",
): Promise<CompareBrandsOutput> {
  const qs = new URLSearchParams({
    brands: brands.join(","),
    window,
  });
  return getJson<CompareBrandsOutput>(`/v1/comparisons?${qs.toString()}`);
}

/** Fetch the tracked-brands roster (primary + competitors) with 7d rollups. */
export async function fetchBrands(): Promise<BrandsResponse> {
  return getJson<BrandsResponse>(`/v1/brands`);
}
