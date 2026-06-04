// Runs feature: live run list/detail + mock-enriched run fetchers.

import { getJson } from "./_client";
import {
  EXTRACTED_MENTIONS,
  RUN_PROVENANCE,
  SAMPLE_RESPONSES,
  genRuns,
  type MockRun,
  type ProvenanceStep,
  type Scenario,
  type ExtractedMention,
} from "../mock";
import type { ProviderId } from "../provider-colors";

export interface RunListRow {
  id: string;
  prompt_name: string;
  provider: string;
  provider_model_version: string;
  started_at: string;
  status: "ok" | "failed";
  error_kind: string | null;
}

export interface RunDetail extends RunListRow {
  prompt_id: string;
  finished_at: string | null;
  raw_response: unknown;
  request_parameters: unknown;
}

export async function fetchRuns(params: {
  limit?: number;
  offset?: number;
}): Promise<{ runs: RunListRow[] }> {
  const qs = new URLSearchParams();
  if (params.limit !== undefined) qs.set("limit", String(params.limit));
  if (params.offset !== undefined) qs.set("offset", String(params.offset));
  const suffix = qs.toString() ? `?${qs.toString()}` : "";
  return getJson(`/api/runs${suffix}`);
}

export async function fetchRunDetail(id: string): Promise<RunDetail> {
  return getJson(`/api/runs/${encodeURIComponent(id)}`);
}

// The set of declared prompts, DB-authoritative via `/v1/prompts`. The
// AddScheduleSheet uses this to populate its prompt picker so an operator can
// schedule a prompt that exists but has never run yet (a fresh project has no
// runs to derive names from). Falls back to the recent-runs names if the
// prompts endpoint is unreachable.
export async function fetchDeclaredPrompts(): Promise<string[]> {
  try {
    const prompts = await getJson<Array<{ name: string }>>("/v1/prompts");
    return Array.from(new Set(prompts.map((p) => p.name))).sort();
  } catch {
    try {
      const r = await fetchRuns({ limit: 200 });
      return Array.from(new Set(r.runs.map((row) => row.prompt_name))).sort();
    } catch {
      return [];
    }
  }
}

/** Mock-enriched runs (adds brand_rank/mentions/latency/tokens). Used by
 *  Overview KPI tiles and the Runs table until the real endpoint returns
 *  these fields. */
export async function fetchEnrichedRuns(
  scenario: Scenario = "healthy",
  limit = 40,
): Promise<{ runs: MockRun[] }> {
  return { runs: genRuns(scenario, limit) };
}

/** Per-run side-by-side raw responses (4 providers). Mock only. */
export async function fetchRunResponses(
  runId: string,
): Promise<{ responses: Readonly<Partial<Record<ProviderId, string>>> }> {
  void runId;
  return { responses: SAMPLE_RESPONSES };
}

/** Per-run extracted mentions matrix (entity x provider). Mock only. */
export async function fetchRunMentions(
  runId: string,
): Promise<{ mentions: Readonly<Partial<Record<ProviderId, ExtractedMention[]>>> }> {
  void runId;
  return { mentions: EXTRACTED_MENTIONS };
}

/** Per-run provenance trail. Mock only. */
export async function fetchRunProvenance(
  runId: string,
): Promise<{ steps: ProvenanceStep[] }> {
  void runId;
  return { steps: [...RUN_PROVENANCE] };
}
