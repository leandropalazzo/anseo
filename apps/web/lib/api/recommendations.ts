// Story 19.8 / 19.9 recommendations surface (live).
//
// Wire envelope mirrors apps/api row_to_json + crates/recommendations wire.rs.
// The NDP invariant is load-bearing for the UI: a recommendation carries the
// `⚠ NDP` marker (UX-DR106) iff reproducibility.class === "non_deterministic",
// which the engine keeps in lockstep with the `non_deterministic_pipeline` tag.

import { getJson } from "./_client";

export type RecommendationSeverity = "info" | "low" | "medium" | "high";
export type RecommendationConfidence = "low" | "medium" | "high";
export type RecommendationState =
  | "generated"
  | "surfaced"
  | "acknowledged"
  | "acted"
  | "measured"
  | "dismissed"
  | "stale";
export type ReproducibilityClass =
  | "byte_stable"
  | "best_effort"
  | "non_deterministic";

export interface RecommendationReproducibility {
  class: ReproducibilityClass;
  note: string | null;
}

export interface RecommendationTraceability {
  source_run_ids: string[];
  source_run_ids_truncated: boolean;
  source_citation_ids: string[];
  source_citation_ids_truncated: boolean;
  source_benchmark_queries: { name: string; query_hash: string }[];
  window: { start: string; end: string };
  input_fingerprint: string;
  llm?: unknown;
}

export interface Recommendation {
  id: string;
  project_id: string;
  kind: string;
  severity: RecommendationSeverity;
  confidence_band: RecommendationConfidence;
  state: RecommendationState;
  summary: string;
  payload: unknown;
  traceability: RecommendationTraceability;
  reproducibility: RecommendationReproducibility;
  tags: string[];
  generated_at: string;
  engine_version: string;
}

export interface RecommendationListResponse {
  items: Recommendation[];
  next_cursor: string | null;
}

/** Priority ordering used to sort recs newest/severest first in the UI. */
export const SEVERITY_RANK: Record<RecommendationSeverity, number> = {
  high: 3,
  medium: 2,
  low: 1,
  info: 0,
};

/** True when the recommendation came off a non-deterministic pipeline and must
 *  carry the `⚠ NDP` marker + suppress hard outcome claims (UX-DR106/109). */
export function isNonDeterministic(rec: Recommendation): boolean {
  return rec.reproducibility.class === "non_deterministic";
}

export interface GenerateResult {
  status: string;
  generated_count: number;
  inserted_count: number;
  status_url: string;
}

/** POST /v1/recommendations/generate — assemble live project facts, run the
 *  in-process engine, and persist (dedup-aware). Mirrors `ogeo recommend
 *  generate`. */
export async function generateRecommendations(): Promise<GenerateResult> {
  // Same-origin proxy → the Next server attaches the operator key (the browser
  // has no API key). See app/api/recommendations/generate/route.ts.
  const r = await fetch(`/api/recommendations/generate`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    cache: "no-store",
  });
  if (!r.ok) throw new Error(`POST generate -> ${r.status} ${r.statusText}`);
  return (await r.json()) as GenerateResult;
}

export interface KindAdoption {
  kind: string;
  surfaced: number;
  acted: number;
  dismissed: number;
  /** acted / surfaced; null when nothing surfaced yet. */
  adoption_rate: number | null;
}

export interface RecommendationIntelligence {
  by_kind: KindAdoption[];
}

/** GET /v1/recommendations/intelligence — per-kind "what works vs what
 *  doesn't": surfaced / acted / dismissed counts driven by the lifecycle. */
export async function fetchRecommendationIntelligence(): Promise<RecommendationIntelligence> {
  return getJson<RecommendationIntelligence>(`/v1/recommendations/intelligence`);
}

export async function fetchRecommendations(params?: {
  limit?: number;
  cursor?: string;
}): Promise<RecommendationListResponse> {
  const qs = new URLSearchParams();
  if (params?.limit !== undefined) qs.set("limit", String(params.limit));
  if (params?.cursor) qs.set("cursor", params.cursor);
  const suffix = qs.toString() ? `?${qs.toString()}` : "";
  return getJson<RecommendationListResponse>(`/v1/recommendations${suffix}`);
}

export async function fetchRecommendationDetail(
  id: string,
): Promise<Recommendation> {
  return getJson<Recommendation>(
    `/v1/recommendations/${encodeURIComponent(id)}`,
  );
}

export interface TransitionResult {
  recommendation: Recommendation;
  warnings: unknown[];
}

/** PATCH /v1/recommendations/:id/state. Snooze (UX-DR104) has no dedicated
 *  lifecycle state, so the UI maps Snooze → acknowledged and Dismiss →
 *  dismissed; mark-acted carries the optional evidence URL for the loop. */
export async function transitionRecommendation(
  id: string,
  body: { to: RecommendationState; note?: string; evidence_url?: string },
): Promise<TransitionResult> {
  // Same-origin proxy → the Next server attaches the operator key.
  const r = await fetch(
    `/api/recommendations/${encodeURIComponent(id)}/state`,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      cache: "no-store",
    },
  );
  if (!r.ok) {
    throw new Error(`PATCH state -> ${r.status} ${r.statusText}`);
  }
  return (await r.json()) as TransitionResult;
}
